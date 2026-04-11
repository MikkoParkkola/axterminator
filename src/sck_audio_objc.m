/// ScreenCaptureKit audio-only capture wrapper for AXTerminator.
///
/// On macOS 14+, SCStream with capturesAudio=true and width=0/height=0
/// captures system audio WITHOUT requiring Screen Recording TCC permission.
/// This is significantly better UX than the AVAudioEngine fallback path.
///
/// Linked weakly — the binary still works on macOS 13 where SCK audio-only
/// is not available (falls back to AVAudioEngine in capture.rs).
///
/// Credit: Matthew Diakonov (@m13v) for the width=0/height=0 technique.
/// Reference: m13v/macos-session-replay ScreenCaptureService.swift

#import <Foundation/Foundation.h>

// Forward-declare ScreenCaptureKit types so we compile even without the
// framework headers on older SDKs.  At runtime we check @available.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wunguarded-availability-new"

@import ScreenCaptureKit;
@import CoreMedia;

#pragma clang diagnostic pop

// ---------------------------------------------------------------------------
// Audio buffer delegate
// ---------------------------------------------------------------------------

@interface AXTSCKAudioDelegate : NSObject <SCStreamOutput>
@property (nonatomic, strong) NSMutableData *pcmData;
@property (nonatomic) double sampleRate;
@property (nonatomic) int channels;
@property (nonatomic) BOOL capturing;
@end

@implementation AXTSCKAudioDelegate

- (instancetype)init {
    self = [super init];
    if (self) {
        _pcmData = [NSMutableData data];
        _sampleRate = 48000.0;
        _channels = 1;
        _capturing = YES;
    }
    return self;
}

- (void)stream:(SCStream *)stream
    didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
                   ofType:(SCStreamOutputType)type
{
    if (!self.capturing) return;

    // Only handle audio buffers.
    if (type != SCStreamOutputTypeAudio) return;

    // Extract audio format from the sample buffer.
    CMFormatDescriptionRef formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer);
    if (formatDesc) {
        const AudioStreamBasicDescription *asbd =
            CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc);
        if (asbd) {
            self.sampleRate = asbd->mSampleRate;
            self.channels = (int)asbd->mChannelsPerFrame;
        }
    }

    // Get the raw audio data block.
    CMBlockBufferRef blockBuffer = CMSampleBufferGetDataBuffer(sampleBuffer);
    if (!blockBuffer) return;

    size_t totalLength = 0;
    char *dataPointer = NULL;
    OSStatus status = CMBlockBufferGetDataPointer(
        blockBuffer, 0, NULL, &totalLength, &dataPointer
    );
    if (status != noErr || !dataPointer || totalLength == 0) return;

    @synchronized (self.pcmData) {
        [self.pcmData appendBytes:dataPointer length:totalLength];
    }
}

@end

// ---------------------------------------------------------------------------
// C-callable interface
// ---------------------------------------------------------------------------

/// Result struct passed back to Rust via FFI.
typedef struct {
    float *samples;       // Caller must free() when done.
    int    sample_count;
    float  sample_rate;
    int    channels;
    int    error_code;    // 0=ok, 1=unavailable, 2=no display, 3=capture failed
    char   error_msg[256];
} AXTSCKCaptureResult;

/// Check if ScreenCaptureKit audio-only capture is available.
///
/// Returns true on macOS 14.0+ where SCStream supports audio-only mode
/// without requiring Screen Recording permission.
bool axt_sck_is_available(void) {
    if (@available(macOS 14.0, *)) {
        // Verify the class is actually loadable.
        Class cls = NSClassFromString(@"SCStream");
        return cls != nil;
    }
    return false;
}

/// Capture system audio for `duration_secs` using ScreenCaptureKit audio-only mode.
///
/// On macOS 14+, this sets SCStreamConfiguration.width=0, height=0 with
/// capturesAudio=true, which avoids the Screen Recording TCC permission dialog.
///
/// ## Thread safety
///
/// ScreenCaptureKit requires that `getShareableContent` and
/// `startCaptureWithCompletionHandler:` be initiated from a thread that has
/// an active CoreFoundation run loop — or equivalently, from a GCD queue that
/// the framework's internal machinery can schedule callbacks back onto.  When
/// called from a raw POSIX/std::thread (as Rust's `std::thread::spawn` creates),
/// SCK returns "Failed due to an invalid parameter" from `startCapture`.
///
/// Fix: perform the two SCK control operations (`getShareableContent` and
/// `startCapture`) synchronously on the main dispatch queue via
/// `dispatch_sync(dispatch_get_main_queue(), …)`.  The calling thread is always
/// a Rust background thread, so dispatching to main never deadlocks.
/// The duration sleep and `stopCapture` remain on a global concurrent queue
/// so we do not block the main run loop for the full capture window.
///
/// The caller is responsible for calling `axt_sck_free_result` to release
/// the returned sample buffer.
AXTSCKCaptureResult axt_sck_capture_system_audio(float duration_secs) {
    AXTSCKCaptureResult result = {0};

    if (!axt_sck_is_available()) {
        result.error_code = 1;
        snprintf(result.error_msg, sizeof(result.error_msg),
                 "ScreenCaptureKit audio-only requires macOS 14.0+");
        return result;
    }

    if (@available(macOS 14.0, *)) {
        // The outer semaphore gates the calling Rust thread until the entire
        // SCK setup, capture, and teardown cycle completes.
        dispatch_semaphore_t doneSem = dispatch_semaphore_create(0);

        // Heap-allocate the result so the main-queue block can write into it
        // after this stack frame has returned (it hasn't — we wait on doneSem —
        // but being explicit about ownership avoids subtle ARC/block-copy bugs).
        __block AXTSCKCaptureResult blockResult = {0};
        float captureDuration = duration_secs;

        // Dispatch the SCK control operations to the main queue.  SCK's
        // completion handlers are delivered on internal GCD queues, but the
        // *initiating* call must come from a proper dispatch context.  The main
        // queue is the simplest correct choice: it always has a run loop, and
        // we are never called from the main thread.
        dispatch_async(dispatch_get_main_queue(), ^{

            // ---- Step 1: Get shareable content (must be on a dispatch queue) ----
            __block SCShareableContent *sharedContent = nil;
            __block NSError *contentError = nil;
            dispatch_semaphore_t contentSem = dispatch_semaphore_create(0);

            [SCShareableContent getShareableContentExcludingDesktopWindows:NO
                                                      onScreenWindowsOnly:YES
                                                        completionHandler:^(SCShareableContent *content,
                                                                            NSError *error) {
                sharedContent = content;
                contentError = error;
                dispatch_semaphore_signal(contentSem);
            }];
            dispatch_semaphore_wait(contentSem,
                                    dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

            if (contentError || !sharedContent || sharedContent.displays.count == 0) {
                blockResult.error_code = 2;
                snprintf(blockResult.error_msg, sizeof(blockResult.error_msg),
                         "No display available for SCK content filter: %s",
                         contentError
                             ? contentError.localizedDescription.UTF8String
                             : "no displays");
                dispatch_semaphore_signal(doneSem);
                return;
            }

            // ---- Step 2: Build content filter ----
            SCDisplay *display = sharedContent.displays.firstObject;
            SCContentFilter *filter =
                [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];

            // ---- Step 3: Configure audio-only stream ----
            // width=0, height=0 suppresses Screen Recording TCC permission.
            SCStreamConfiguration *config = [[SCStreamConfiguration alloc] init];
            config.width = 0;
            config.height = 0;
            config.capturesAudio = YES;
            config.excludesCurrentProcessAudio = YES;
            config.channelCount = 1;   // Mono — matches our 16 kHz mono pipeline.
            config.sampleRate = 48000; // Capture at 48 kHz, Rust will downsample.

            // ---- Step 4: Create stream and attach delegate ----
            SCStream *stream = [[SCStream alloc] initWithFilter:filter
                                                  configuration:config
                                                       delegate:nil];

            AXTSCKAudioDelegate *delegate = [[AXTSCKAudioDelegate alloc] init];
            NSError *streamError = nil;
            BOOL added = [stream addStreamOutput:delegate
                                            type:SCStreamOutputTypeAudio
                              sampleHandlerQueue:dispatch_get_global_queue(
                                                     QOS_CLASS_USER_INTERACTIVE, 0)
                                           error:&streamError];
            if (!added || streamError) {
                blockResult.error_code = 3;
                snprintf(blockResult.error_msg, sizeof(blockResult.error_msg),
                         "Failed to add stream output: %s",
                         streamError
                             ? streamError.localizedDescription.UTF8String
                             : "unknown");
                dispatch_semaphore_signal(doneSem);
                return;
            }

            // ---- Step 5: Start capture (also requires a proper dispatch context) ----
            __block NSError *startError = nil;
            dispatch_semaphore_t startSem = dispatch_semaphore_create(0);
            [stream startCaptureWithCompletionHandler:^(NSError *error) {
                startError = error;
                dispatch_semaphore_signal(startSem);
            }];
            dispatch_semaphore_wait(startSem,
                                    dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

            if (startError) {
                blockResult.error_code = 3;
                snprintf(blockResult.error_msg, sizeof(blockResult.error_msg),
                         "SCStream start failed: %s",
                         startError.localizedDescription.UTF8String);
                dispatch_semaphore_signal(doneSem);
                return;
            }

            // ---- Step 6 & 7: Sleep + stop on a global queue ----
            // Move off the main queue for the blocking sleep so the main run
            // loop remains responsive during the capture window.
            dispatch_async(dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^{

                [NSThread sleepForTimeInterval:(NSTimeInterval)captureDuration];

                // Signal the delegate to stop accumulating samples.
                delegate.capturing = NO;

                // ---- Step 7: Stop capture ----
                dispatch_semaphore_t stopSem = dispatch_semaphore_create(0);
                [stream stopCaptureWithCompletionHandler:^(NSError * _Nullable __unused e) {
                    dispatch_semaphore_signal(stopSem);
                }];
                dispatch_semaphore_wait(stopSem,
                                        dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

                // ---- Step 8: Copy PCM data out for Rust ----
                NSData *pcmBytes;
                @synchronized (delegate.pcmData) {
                    pcmBytes = [delegate.pcmData copy];
                }

                if (pcmBytes.length == 0) {
                    // Silence or no system audio — not an error.
                    blockResult.sample_count = 0;
                    blockResult.samples = NULL;
                    blockResult.sample_rate = (float)delegate.sampleRate;
                    blockResult.channels = delegate.channels;
                    blockResult.error_code = 0;
                    dispatch_semaphore_signal(doneSem);
                    return;
                }

                // SCK delivers 32-bit float PCM (kAudioFormatFlagIsFloat).
                int float_count = (int)(pcmBytes.length / sizeof(float));
                float *out = (float *)malloc((size_t)float_count * sizeof(float));
                if (!out) {
                    blockResult.error_code = 3;
                    snprintf(blockResult.error_msg, sizeof(blockResult.error_msg),
                             "malloc failed for audio buffer");
                    dispatch_semaphore_signal(doneSem);
                    return;
                }
                memcpy(out, pcmBytes.bytes, (size_t)float_count * sizeof(float));

                blockResult.samples = out;
                blockResult.sample_count = float_count;
                blockResult.sample_rate = (float)delegate.sampleRate;
                blockResult.channels = delegate.channels;
                blockResult.error_code = 0;
                dispatch_semaphore_signal(doneSem);
            }); // end global-queue block
        }); // end main-queue block

        // Block the calling Rust thread until the full capture cycle finishes.
        // Timeout: duration + 15 s headroom for startup/teardown.
        dispatch_time_t timeout = dispatch_time(
            DISPATCH_TIME_NOW,
            (int64_t)((captureDuration + 15.0f) * (float)NSEC_PER_SEC));
        long waited = dispatch_semaphore_wait(doneSem, timeout);
        if (waited != 0) {
            // Timed out — the async block may still be running.  Mark the
            // result as an error so the Rust caller does not consume stale
            // or partially-written data.
            blockResult.error_code = 1;
            snprintf(blockResult.error_msg, sizeof(blockResult.error_msg),
                     "System audio capture timed out");
        }

        result = blockResult;
        return result;
    }

    // Should not reach here due to axt_sck_is_available() guard.
    result.error_code = 1;
    snprintf(result.error_msg, sizeof(result.error_msg), "Unreachable: macOS version check failed");
    return result;
}

/// Free the sample buffer allocated by `axt_sck_capture_system_audio`.
void axt_sck_free_result(AXTSCKCaptureResult *result) {
    if (result && result->samples) {
        free(result->samples);
        result->samples = NULL;
        result->sample_count = 0;
    }
}
