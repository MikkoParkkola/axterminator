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
        // Synchronisation: we block the calling thread while async SCK ops complete.
        dispatch_semaphore_t sem = dispatch_semaphore_create(0);

        __block SCShareableContent *sharedContent = nil;
        __block NSError *contentError = nil;

        // 1. Get shareable content (displays).
        [SCShareableContent getShareableContentExcludingDesktopWindows:NO
                                                  onScreenWindowsOnly:YES
                                                    completionHandler:^(SCShareableContent *content,
                                                                        NSError *error) {
            sharedContent = content;
            contentError = error;
            dispatch_semaphore_signal(sem);
        }];
        dispatch_semaphore_wait(sem, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

        if (contentError || !sharedContent || sharedContent.displays.count == 0) {
            result.error_code = 2;
            snprintf(result.error_msg, sizeof(result.error_msg),
                     "No display available for SCK content filter: %s",
                     contentError ? contentError.localizedDescription.UTF8String : "no displays");
            return result;
        }

        // 2. Create content filter from the first display.
        SCDisplay *display = sharedContent.displays.firstObject;
        SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:display
                                                          excludingWindows:@[]];

        // 3. Configure audio-only stream: width=0, height=0 avoids Screen Recording permission.
        SCStreamConfiguration *config = [[SCStreamConfiguration alloc] init];
        config.width = 0;
        config.height = 0;
        config.capturesAudio = YES;
        config.excludesCurrentProcessAudio = YES;
        config.channelCount = 1;       // Mono — matches our 16kHz mono pipeline.
        config.sampleRate = 48000;     // Capture at 48kHz, Rust will downsample.

        // 4. Create stream and delegate.
        NSError *streamError = nil;
        SCStream *stream = [[SCStream alloc] initWithFilter:filter
                                              configuration:config
                                                   delegate:nil];

        AXTSCKAudioDelegate *delegate = [[AXTSCKAudioDelegate alloc] init];
        BOOL added = [stream addStreamOutput:delegate
                                        type:SCStreamOutputTypeAudio
                          sampleHandlerQueue:dispatch_get_global_queue(QOS_CLASS_USER_INTERACTIVE, 0)
                                       error:&streamError];
        if (!added || streamError) {
            result.error_code = 3;
            snprintf(result.error_msg, sizeof(result.error_msg),
                     "Failed to add stream output: %s",
                     streamError ? streamError.localizedDescription.UTF8String : "unknown");
            return result;
        }

        // 5. Start capture.
        __block NSError *startError = nil;
        dispatch_semaphore_t startSem = dispatch_semaphore_create(0);
        [stream startCaptureWithCompletionHandler:^(NSError *error) {
            startError = error;
            dispatch_semaphore_signal(startSem);
        }];
        dispatch_semaphore_wait(startSem, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

        if (startError) {
            result.error_code = 3;
            snprintf(result.error_msg, sizeof(result.error_msg),
                     "SCStream start failed: %s", startError.localizedDescription.UTF8String);
            return result;
        }

        // 6. Capture for the requested duration.
        [NSThread sleepForTimeInterval:(NSTimeInterval)duration_secs];

        // 7. Stop capture.
        delegate.capturing = NO;
        dispatch_semaphore_t stopSem = dispatch_semaphore_create(0);
        [stream stopCaptureWithCompletionHandler:^(NSError * _Nullable __unused stopError) {
            dispatch_semaphore_signal(stopSem);
        }];
        dispatch_semaphore_wait(stopSem, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));

        // 8. Convert captured PCM data to float samples for Rust.
        NSData *pcmBytes;
        @synchronized (delegate.pcmData) {
            pcmBytes = [delegate.pcmData copy];
        }

        if (pcmBytes.length == 0) {
            // No audio captured — might be silence or no system audio playing.
            result.sample_count = 0;
            result.samples = NULL;
            result.sample_rate = (float)delegate.sampleRate;
            result.channels = delegate.channels;
            result.error_code = 0;
            return result;
        }

        // SCK delivers audio as 32-bit float PCM (kAudioFormatFlagIsFloat).
        int float_count = (int)(pcmBytes.length / sizeof(float));
        float *out = (float *)malloc(float_count * sizeof(float));
        if (!out) {
            result.error_code = 3;
            snprintf(result.error_msg, sizeof(result.error_msg), "malloc failed for audio buffer");
            return result;
        }
        memcpy(out, pcmBytes.bytes, float_count * sizeof(float));

        result.samples = out;
        result.sample_count = float_count;
        result.sample_rate = (float)delegate.sampleRate;
        result.channels = delegate.channels;
        result.error_code = 0;
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
