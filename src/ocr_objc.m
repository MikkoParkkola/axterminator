/**
 * ocr_objc.m — VNRecognizeTextRequest OCR bindings for ocr.rs
 *
 * Provides on-device OCR for the AX-tree OCR fallback path.
 * No TCC permission is required — Vision text recognition operates on
 * pixel data that the caller has already captured via CGWindowListCreateImage
 * (which is gated by Screen Recording only when targeting other processes,
 * and is already used by the existing screenshot path).
 *
 * Public C API:
 *   axt_recognize_text(png_data, png_len, min_confidence) — PNG → C string
 *   axt_free_text(text)                                   — free result
 *
 * Availability: VNRecognizeTextRequest requires macOS 10.15+.
 * The function returns an empty string on older systems rather than crashing.
 *
 * Threading: safe to call from any thread.  The Vision request handler
 * creates its own serial dispatch queue internally.
 *
 * Memory: the returned string is malloc'd; caller must call axt_free_text.
 */

#import <Vision/Vision.h>
#import <CoreImage/CoreImage.h>
#import <Foundation/Foundation.h>

// ---------------------------------------------------------------------------
// axt_recognize_text
// ---------------------------------------------------------------------------

/**
 * Run VNRecognizeTextRequest on a PNG image and return recognised text.
 *
 * @param png_data  Pointer to PNG-encoded image bytes.
 * @param png_len   Length of the PNG data in bytes.
 * @param min_confidence  Minimum per-observation confidence threshold [0.0, 1.0].
 *                        Observations below this threshold are skipped.
 * @return  Heap-allocated NUL-terminated UTF-8 string with all recognised text
 *          joined by newlines.  Empty string ("") when no text is found or
 *          the framework is unavailable.  Never returns NULL.
 *          Caller must free via axt_free_text().
 */
const char *axt_recognize_text(const uint8_t *png_data,
                                size_t         png_len,
                                float          min_confidence) {
    @autoreleasepool {
        if (!png_data || png_len == 0) {
            return strdup("");
        }

        if (@available(macOS 10.15, *)) {
            NSData *data = [NSData dataWithBytesNoCopy:(void *)png_data
                                               length:png_len
                                         freeWhenDone:NO];
            CIImage *ciImage = [CIImage imageWithData:data];
            if (!ciImage) {
                return strdup("");
            }

            VNRecognizeTextRequest *req = [[VNRecognizeTextRequest alloc] init];
            req.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
            req.usesLanguageCorrection = YES;

            VNImageRequestHandler *handler =
                [[VNImageRequestHandler alloc] initWithCIImage:ciImage options:@{}];
            NSError *err = nil;
            [handler performRequests:@[req] error:&err];

            if (err || !req.results || req.results.count == 0) {
                return strdup("");
            }

            NSMutableArray<NSString *> *lines = [NSMutableArray array];
            for (VNRecognizedTextObservation *obs in req.results) {
                VNRecognizedText *top = [obs topCandidates:1].firstObject;
                if (!top || top.confidence < min_confidence) continue;
                [lines addObject:top.string];
            }

            if (lines.count == 0) {
                return strdup("");
            }

            NSString *joined = [lines componentsJoinedByString:@"\n"];
            return strdup(joined.UTF8String ?: "");
        } else {
            return strdup("");
        }
    }
}

// ---------------------------------------------------------------------------
// axt_free_text
// ---------------------------------------------------------------------------

/**
 * Free a string returned by axt_recognize_text.
 *
 * Safe to call with NULL.
 */
void axt_free_text(const char *text) {
    free((void *)text);
}
