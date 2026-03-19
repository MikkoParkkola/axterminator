/**
 * camera_objc.m — AVFoundation + Vision framework bindings for camera.rs
 *
 * Compiled only when the `camera` feature is enabled (see build.rs).
 *
 * Public C API:
 *   av_camera_authorization_status()  — TCC authorization check
 *   av_list_cameras()                  — enumerate devices
 *   av_free_camera_list()              — free enumeration result
 *   av_capture_frame()                 — single-frame JPEG capture
 *   av_free_frame_result()             — free capture result
 *   vn_detect_gestures()               — Vision hand-pose + face detection
 *   vn_free_gesture_list()             — free detection result
 *
 * Threading: all functions are called from the Rust thread pool. AVFoundation
 * dispatch queues are created internally as private serial queues so there
 * is no interaction with the main run loop.
 *
 * Privacy: AVCaptureSession is started immediately before grabbing a single
 * sample buffer and stopped + released immediately after. The camera indicator
 * light will be ON for the duration of the capture (~100–500 ms). This
 * satisfies AC8 (no persistent camera access).
 */

#import <AVFoundation/AVFoundation.h>
#import <Vision/Vision.h>
#import <CoreImage/CoreImage.h>
#import <ImageIO/ImageIO.h>
#import <Foundation/Foundation.h>

// ---------------------------------------------------------------------------
// C-compatible structs (mirrored in camera.rs)
// ---------------------------------------------------------------------------

typedef struct {
    const char *unique_id;       /* heap-allocated via strdup */
    const char *localized_name;  /* heap-allocated via strdup */
    int         position;        /* 1=front, 2=back, 3=external, 0=unknown */
    int         is_default;      /* 1 if system default, 0 otherwise */
} CDeviceInfo;

typedef struct {
    void   *jpeg_data;   /* heap-allocated malloc'd block */
    size_t  jpeg_len;
    uint32_t width;
    uint32_t height;
    const char *error_msg; /* heap-allocated via strdup, or NULL */
} CFrameResult;

typedef struct {
    const char *gesture_name; /* static string literal — NOT heap-allocated */
    float       confidence;
    int         hand_code;    /* 0=left, 1=right, 2=face, 3=unknown */
} CGestureItem;

typedef struct {
    CGestureItem *items;       /* heap-allocated array */
    size_t        count;
    const char   *error_msg;   /* heap-allocated via strdup, or NULL */
} CGestureList;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static int position_code(AVCaptureDevicePosition pos) {
    switch (pos) {
        case AVCaptureDevicePositionFront:   return 1;
        case AVCaptureDevicePositionBack:    return 2;
        default:                             return 3; /* treat unspecified as external */
    }
}

// ---------------------------------------------------------------------------
// av_camera_authorization_status
// ---------------------------------------------------------------------------

int av_camera_authorization_status(void) {
    return (int)[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo];
}

int av_request_camera_access(void) {
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo];
    if (status == AVAuthorizationStatusAuthorized) return 1;
    if (status != AVAuthorizationStatusNotDetermined) return 0;

    __block BOOL granted = NO;
    dispatch_semaphore_t sema = dispatch_semaphore_create(0);
    [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL g) {
        granted = g;
        dispatch_semaphore_signal(sema);
    }];
    dispatch_semaphore_wait(sema, dispatch_time(DISPATCH_TIME_NOW, 30 * NSEC_PER_SEC));
    return granted ? 1 : 0;
}

// ---------------------------------------------------------------------------
// av_list_cameras / av_free_camera_list
// ---------------------------------------------------------------------------

CDeviceInfo *av_list_cameras(size_t *out_count) {
    NSArray<AVCaptureDevice *> *devices;
    if (@available(macOS 10.15, *)) {
        AVCaptureDeviceDiscoverySession *session =
            [AVCaptureDeviceDiscoverySession
                discoverySessionWithDeviceTypes:@[
                    AVCaptureDeviceTypeBuiltInWideAngleCamera,
                    AVCaptureDeviceTypeExternalUnknown
                ]
                mediaType:AVMediaTypeVideo
                position:AVCaptureDevicePositionUnspecified];
        devices = session.devices;
    } else {
        devices = [AVCaptureDevice devicesWithMediaType:AVMediaTypeVideo];
    }

    *out_count = (size_t)devices.count;
    if (*out_count == 0) return NULL;

    CDeviceInfo *list = calloc(*out_count, sizeof(CDeviceInfo));
    if (!list) { *out_count = 0; return NULL; }

    AVCaptureDevice *defaultDevice = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];

    for (NSUInteger i = 0; i < devices.count; i++) {
        AVCaptureDevice *dev = devices[i];
        list[i].unique_id       = strdup(dev.uniqueID.UTF8String);
        list[i].localized_name  = strdup(dev.localizedName.UTF8String);
        list[i].position        = position_code(dev.position);
        list[i].is_default      = [dev.uniqueID isEqualToString:defaultDevice.uniqueID] ? 1 : 0;
    }
    return list;
}

void av_free_camera_list(CDeviceInfo *list, size_t count) {
    if (!list) return;
    for (size_t i = 0; i < count; i++) {
        free((void *)list[i].unique_id);
        free((void *)list[i].localized_name);
    }
    free(list);
}

// ---------------------------------------------------------------------------
// av_capture_frame / av_free_frame_result
// ---------------------------------------------------------------------------

/**
 * CameraFrameDelegate — sample buffer delegate that captures exactly one
 * frame then signals a semaphore so the calling thread can resume.
 */
@interface CameraFrameDelegate : NSObject <AVCaptureVideoDataOutputSampleBufferDelegate>
@property (nonatomic, strong) dispatch_semaphore_t semaphore;
@property (nonatomic, assign) CMSampleBufferRef capturedBuffer; /* +1 retain */
@end

@implementation CameraFrameDelegate

- (void)captureOutput:(AVCaptureOutput *)output
    didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
           fromConnection:(AVCaptureConnection *)connection {
    if (self.capturedBuffer) return; /* already have one */
    CFRetain(sampleBuffer);
    self.capturedBuffer = sampleBuffer;
    dispatch_semaphore_signal(self.semaphore);
}

@end

static NSData *encode_pixel_buffer_as_jpeg(CVPixelBufferRef pixelBuffer,
                                            uint32_t *out_width,
                                            uint32_t *out_height) {
    CIImage *ciImage = [CIImage imageWithCVPixelBuffer:pixelBuffer];
    if (!ciImage) return nil;

    *out_width  = (uint32_t)CVPixelBufferGetWidth(pixelBuffer);
    *out_height = (uint32_t)CVPixelBufferGetHeight(pixelBuffer);

    NSDictionary *options = @{kCIContextUseSoftwareRenderer: @NO};
    CIContext *ctx = [CIContext contextWithOptions:options];

    NSMutableData *data = [NSMutableData data];
    CGImageDestinationRef dest = CGImageDestinationCreateWithData(
        (CFMutableDataRef)data,
        kUTTypeJPEG,
        1,
        NULL);
    if (!dest) return nil;

    CGImageRef cgImage = [ctx createCGImage:ciImage fromRect:ciImage.extent];
    if (!cgImage) { CFRelease(dest); return nil; }

    NSDictionary *props = @{(NSString *)kCGImageDestinationLossyCompressionQuality: @0.90};
    CGImageDestinationAddImage(dest, cgImage, (CFDictionaryRef)props);
    bool ok = CGImageDestinationFinalize(dest);
    CGImageRelease(cgImage);
    CFRelease(dest);

    return ok ? data : nil;
}

bool av_capture_frame(const char *device_id_cstr, CFrameResult *result) {
    @autoreleasepool {
        /* --- select device --- */
        AVCaptureDevice *device = nil;
        if (device_id_cstr) {
            NSString *wantedId = [NSString stringWithUTF8String:device_id_cstr];
            device = [AVCaptureDevice deviceWithUniqueID:wantedId];
        }
        if (!device) {
            device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
        }
        if (!device) {
            result->error_msg = strdup("No video capture device available");
            return false;
        }

        /* --- build session --- */
        AVCaptureSession *session = [[AVCaptureSession alloc] init];
        session.sessionPreset = AVCaptureSessionPreset1280x720;

        NSError *error = nil;
        AVCaptureDeviceInput *input =
            [AVCaptureDeviceInput deviceInputWithDevice:device error:&error];
        if (!input || error) {
            result->error_msg = strdup(error.localizedDescription.UTF8String ?: "Input init failed");
            return false;
        }
        if (![session canAddInput:input]) {
            result->error_msg = strdup("Cannot add device input to session");
            return false;
        }
        [session addInput:input];

        /* --- output --- */
        AVCaptureVideoDataOutput *output = [[AVCaptureVideoDataOutput alloc] init];
        output.videoSettings = @{
            (NSString *)kCVPixelBufferPixelFormatTypeKey:
                @(kCVPixelFormatType_32BGRA)
        };
        output.alwaysDiscardsLateVideoFrames = YES;

        dispatch_queue_t queue =
            dispatch_queue_create("ax.camera.capture", DISPATCH_QUEUE_SERIAL);

        CameraFrameDelegate *delegate = [[CameraFrameDelegate alloc] init];
        delegate.semaphore = dispatch_semaphore_create(0);
        [output setSampleBufferDelegate:delegate queue:queue];

        if (![session canAddOutput:output]) {
            result->error_msg = strdup("Cannot add video output to session");
            return false;
        }
        [session addOutput:output];

        /* --- capture one frame --- */
        [session startRunning];

        /* Wait up to 5 s for the first frame. */
        dispatch_time_t timeout = dispatch_time(DISPATCH_TIME_NOW, 5LL * NSEC_PER_SEC);
        long waited = dispatch_semaphore_wait(delegate.semaphore, timeout);

        [session stopRunning];
        [output setSampleBufferDelegate:nil queue:nil];

        if (waited != 0 || !delegate.capturedBuffer) {
            result->error_msg = strdup("Timed out waiting for camera frame");
            return false;
        }

        /* --- encode to JPEG --- */
        CVPixelBufferRef pixelBuffer =
            CMSampleBufferGetImageBuffer(delegate.capturedBuffer);

        uint32_t width = 0, height = 0;
        NSData *jpeg = encode_pixel_buffer_as_jpeg(pixelBuffer, &width, &height);
        CFRelease(delegate.capturedBuffer);
        delegate.capturedBuffer = nil;

        if (!jpeg || jpeg.length == 0) {
            result->error_msg = strdup("Failed to encode frame as JPEG");
            return false;
        }

        /* Copy JPEG bytes into malloc'd block owned by the caller. */
        void *buf = malloc(jpeg.length);
        if (!buf) {
            result->error_msg = strdup("Out of memory");
            return false;
        }
        memcpy(buf, jpeg.bytes, jpeg.length);

        result->jpeg_data = buf;
        result->jpeg_len  = jpeg.length;
        result->width     = width;
        result->height    = height;
        result->error_msg = NULL;
        return true;
    }
}

void av_free_frame_result(CFrameResult *result) {
    if (!result) return;
    free(result->jpeg_data);
    free((void *)result->error_msg);
    result->jpeg_data  = NULL;
    result->error_msg  = NULL;
}

// ---------------------------------------------------------------------------
// vn_detect_gestures / vn_free_gesture_list
// ---------------------------------------------------------------------------

/**
 * Map VNHumanHandPoseObservation joint angles to gesture names.
 *
 * Strategy: inspect the y-coordinates of key landmarks relative to the wrist
 * to classify simple gestures without a trained classifier, matching the
 * heuristics Vision itself uses for the built-in recogniser on iOS 17+.
 *
 * Returns a static string literal (no heap allocation).
 */
/**
 * Minimum joint confidence to trust a landmark position.
 * Vision can return a point with confidence=0 when the joint is occluded;
 * using those positions produces false classifications.
 */
static const float kMinJointConf = 0.15f;

/**
 * Thresholds for "finger extended" / "finger curled" decisions.
 *
 * Vision returns normalized image coordinates (x,y ∈ [0,1]) with y=0 at the
 * bottom of the image and y=1 at the top.  For a hand held vertically in front
 * of the camera the wrist is near y=0.2 and extended fingertips reach y=0.7+,
 * giving a delta of ~0.5.  We use a conservative 0.08 so the detector fires
 * even when the hand is partially off-frame or held at an angle.
 *
 * The thumb uses a slightly larger threshold (0.10) because the thumb's
 * neutral position is already elevated relative to the wrist.
 */
static const float kFingerUpThresh  = 0.08f;  /* index/middle/ring/little */
static const float kThumbUpThresh   = 0.10f;  /* thumb tip above wrist    */
static const float kThumbDownThresh = 0.10f;  /* thumb tip below wrist    */

static const char *classify_hand_pose(VNHumanHandPoseObservation *obs, float *confidence) API_AVAILABLE(macos(11.0)) {
    NSError *err = nil;

    /* Collect fingertip and base joint points */
    VNRecognizedPoint *wrist =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameWrist error:&err];
    VNRecognizedPoint *thumbTip =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameThumbTip error:&err];
    VNRecognizedPoint *indexTip =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameIndexTip error:&err];
    VNRecognizedPoint *middleTip =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameMiddleTip error:&err];
    VNRecognizedPoint *ringTip =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameRingTip error:&err];
    VNRecognizedPoint *littleTip =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameLittleTip error:&err];
    VNRecognizedPoint *thumbIP =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameThumbIP error:&err];
    VNRecognizedPoint *indexMCP =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameIndexMCP error:&err];
    VNRecognizedPoint *middleMCP =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameMiddleMCP error:&err];
    VNRecognizedPoint *ringMCP =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameRingMCP error:&err];
    VNRecognizedPoint *littleMCP =
        [obs recognizedPointForJointName:VNHumanHandPoseObservationJointNameLittleMCP error:&err];

    if (!wrist || wrist.confidence < kMinJointConf) {
        *confidence = 0.0f;
        return NULL;
    }

    float wy = (float)wrist.location.y;
    float wx = (float)wrist.location.x;

    /* Debug: log raw joint positions so unexpected 0-detection can be diagnosed */
    NSLog(@"[axterminator] hand-pose wrist=(%.3f,%.3f,conf=%.2f) "
          @"thumbTip=(%.3f,%.3f,conf=%.2f) indexTip=(%.3f,%.3f,conf=%.2f) "
          @"middleTip=(%.3f,%.3f,conf=%.2f) ringTip=(%.3f,%.3f,conf=%.2f) "
          @"littleTip=(%.3f,%.3f,conf=%.2f) obs.confidence=%.2f",
          wx, wy, (float)wrist.confidence,
          thumbTip  ? (float)thumbTip.location.x  : -1.f,
          thumbTip  ? (float)thumbTip.location.y  : -1.f,
          thumbTip  ? (float)thumbTip.confidence  : -1.f,
          indexTip  ? (float)indexTip.location.x  : -1.f,
          indexTip  ? (float)indexTip.location.y  : -1.f,
          indexTip  ? (float)indexTip.confidence  : -1.f,
          middleTip ? (float)middleTip.location.x : -1.f,
          middleTip ? (float)middleTip.location.y : -1.f,
          middleTip ? (float)middleTip.confidence : -1.f,
          ringTip   ? (float)ringTip.location.x   : -1.f,
          ringTip   ? (float)ringTip.location.y   : -1.f,
          ringTip   ? (float)ringTip.confidence   : -1.f,
          littleTip ? (float)littleTip.location.x : -1.f,
          littleTip ? (float)littleTip.location.y : -1.f,
          littleTip ? (float)littleTip.confidence : -1.f,
          (float)obs.confidence);

    /**
     * "Is a joint confident enough to use?"
     * Returns YES when the point is non-nil and its Vision confidence exceeds
     * kMinJointConf.  Joints occluded by other fingers or out-of-frame return
     * confidence ~0 and should be treated as unknown, not as "low".
     */
#define JOINT_OK(pt) ((pt) && (float)(pt).confidence >= kMinJointConf)

    /**
     * "Is this fingertip above its MCP knuckle?"
     *
     * Comparing tip to its own MCP (instead of the wrist) is more robust for
     * hands that are angled or partially off-frame: the MCP moves with the
     * hand, so the relative delta is stable even when the absolute y-positions
     * shift.  Fall back to a wrist-relative check when the MCP is occluded.
     */
#define TIP_ABOVE_MCP(tip, mcp) \
    (JOINT_OK(tip) && JOINT_OK(mcp) \
        ? (float)(tip).location.y > (float)(mcp).location.y + kFingerUpThresh \
        : (JOINT_OK(tip) && (float)(tip).location.y > wy + kFingerUpThresh))

    /* Vision coords: y=0 bottom, y=1 top.  Higher y = finger extended upward. */
    bool thumbUp   = JOINT_OK(thumbTip) && (float)thumbTip.location.y  > wy + kThumbUpThresh;
    bool thumbDown = JOINT_OK(thumbTip) && (float)thumbTip.location.y  < wy - kThumbDownThresh;
    bool indexUp   = TIP_ABOVE_MCP(indexTip,  indexMCP);
    bool middleUp  = TIP_ABOVE_MCP(middleTip, middleMCP);
    bool ringUp    = TIP_ABOVE_MCP(ringTip,   ringMCP);
    bool littleUp  = TIP_ABOVE_MCP(littleTip, littleMCP);

    NSLog(@"[axterminator] gesture flags thumbUp=%d thumbDown=%d indexUp=%d "
          @"middleUp=%d ringUp=%d littleUp=%d",
          thumbUp, thumbDown, indexUp, middleUp, ringUp, littleUp);

    /* Thumb extended downward and all others closed */
    bool allFingersDown = !indexUp && !middleUp && !ringUp && !littleUp;

    /* Detect open palm: all 5 fingers extended */
    if (thumbUp && indexUp && middleUp && ringUp && littleUp) {
        *confidence = (float)MIN(obs.confidence, 0.95);
        return "stop";
    }

    /* Thumbs up: thumb up, all others closed */
    if (thumbUp && allFingersDown) {
        *confidence = (float)MIN(thumbTip.confidence, 0.90);
        return "thumbs_up";
    }

    /* Thumbs down: thumb down, all others closed */
    if (thumbDown && allFingersDown) {
        bool ipDown = thumbIP && (float)thumbIP.location.y < wy - 0.10f;
        *confidence = ipDown ? (float)MIN(thumbTip.confidence, 0.85) : 0.60f;
        return "thumbs_down";
    }

    /* Point: index only */
    if (indexUp && !middleUp && !ringUp && !littleUp) {
        *confidence = (float)MIN(indexTip.confidence, 0.88);
        return "point";
    }

    /* Wave: index + middle extended (V / victory sign also maps here) */
    if (indexUp && middleUp && !ringUp && !littleUp) {
        *confidence = 0.80f;
        return "wave";
    }

    return NULL;
}

bool vn_detect_gestures(const uint8_t *jpeg_data,
                        size_t         jpeg_len,
                        CGestureList  *list) {
    @autoreleasepool {
        list->items     = NULL;
        list->count     = 0;
        list->error_msg = NULL;

        NSData *data = [NSData dataWithBytesNoCopy:(void *)jpeg_data
                                            length:jpeg_len
                                      freeWhenDone:NO];
        CIImage *ciImage = [CIImage imageWithData:data];
        if (!ciImage) {
            list->error_msg = strdup("Cannot decode JPEG for Vision processing");
            return false;
        }

        /* Maximum capacity: one hand result + one face result */
        CGestureItem *items = calloc(2, sizeof(CGestureItem));
        if (!items) {
            list->error_msg = strdup("Out of memory");
            return false;
        }
        size_t found = 0;

        if (@available(macOS 11.0, *)) {
            /* --- Hand pose --- */
            VNDetectHumanHandPoseRequest *handReq =
                [[VNDetectHumanHandPoseRequest alloc] init];
            handReq.maximumHandCount = 2;

            VNImageRequestHandler *handler =
                [[VNImageRequestHandler alloc] initWithCIImage:ciImage options:@{}];
            NSError *err = nil;
            [handler performRequests:@[handReq] error:&err];

            NSArray<VNHumanHandPoseObservation *> *observations = handReq.results;
            for (VNHumanHandPoseObservation *obs in observations) {
                if (found >= 2) break;

                float confidence = 0.0f;
                const char *name = classify_hand_pose(obs, &confidence);
                if (!name || confidence < 0.5f) continue;

                items[found].gesture_name = name; /* static literal */
                items[found].confidence   = confidence;
                /* chirality: left=0, right=1, unknown=3 */
                if (@available(macOS 12.0, *)) {
                    switch (obs.chirality) {
                        case VNChiralityLeft:  items[found].hand_code = 0; break;
                        case VNChiralityRight: items[found].hand_code = 1; break;
                        default:               items[found].hand_code = 3; break;
                    }
                } else {
                    items[found].hand_code = 3;
                }
                found++;
            }

            /* --- Face landmarks (nod / shake) --- */
            VNDetectFaceLandmarksRequest *faceReq =
                [[VNDetectFaceLandmarksRequest alloc] init];
            VNImageRequestHandler *faceHandler =
                [[VNImageRequestHandler alloc] initWithCIImage:ciImage options:@{}];
            NSError *faceErr = nil;
            [faceHandler performRequests:@[faceReq] error:&faceErr];

            /* We detect nod/shake via roll and yaw angles on macOS 12+ */
            if (@available(macOS 12.0, *)) {
                for (VNFaceObservation *face in faceReq.results) {
                    if (found >= 2) break;
                    if (!face.pitch || !face.yaw) continue;

                    float pitch = (float)face.pitch.doubleValue; /* radians */
                    float yaw   = (float)face.yaw.doubleValue;

                    if (fabsf(pitch) > 0.35f && fabsf(pitch) > fabsf(yaw)) {
                        items[found].gesture_name = "nod";
                        items[found].confidence   = MIN(0.75f + fabsf(pitch) * 0.5f, 0.95f);
                        items[found].hand_code    = 2; /* face */
                        found++;
                    } else if (fabsf(yaw) > 0.35f && fabsf(yaw) > fabsf(pitch)) {
                        items[found].gesture_name = "shake";
                        items[found].confidence   = MIN(0.75f + fabsf(yaw) * 0.5f, 0.95f);
                        items[found].hand_code    = 2; /* face */
                        found++;
                    }
                }
            }
        } else {
            /* macOS < 11: Vision hand pose not available */
            free(items);
            list->error_msg = strdup("VNDetectHumanHandPoseRequest requires macOS 11.0 or later");
            return false;
        }

        if (found == 0) {
            free(items);
            list->items = NULL;
        } else {
            list->items = items;
        }
        list->count = found;
        return true;
    }
}

void vn_free_gesture_list(CGestureList *list) {
    if (!list) return;
    /* gesture_name strings are static literals — do NOT free them */
    free(list->items);
    free((void *)list->error_msg);
    list->items     = NULL;
    list->error_msg = NULL;
    list->count     = 0;
}
