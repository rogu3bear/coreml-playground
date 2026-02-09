// CoreMLBridge.swift
// C-compatible FFI layer for CoreML model loading, metadata extraction, and inference.
//
// All public functions use @_cdecl for stable C symbol names, accept and return
// only C-compatible types (raw pointers, CChar buffers, integers), and never throw.
// Errors are surfaced as JSON strings: {"error": "..."}.

import Foundation
import CoreML
import AppKit
import CoreGraphics
import CoreVideo

// MARK: - Internal State

/// Thread-safe wrapper around a compiled MLModel and its source path.
private final class ModelHandle {
    let model: MLModel
    let sourceURL: URL

    init(model: MLModel, sourceURL: URL) {
        self.model = model
        self.sourceURL = sourceURL
    }
}

// MARK: - Helpers

/// Allocate a C string on the heap that the caller must free with `coreml_free_string`.
private func allocateCString(_ string: String) -> UnsafePointer<CChar> {
    let utf8 = string.utf8CString
    let buffer = UnsafeMutableBufferPointer<CChar>.allocate(capacity: utf8.count)
    _ = buffer.initialize(from: utf8)
    return UnsafePointer(buffer.baseAddress!)
}

/// Build a JSON error response.
private func errorJSON(_ message: String) -> UnsafePointer<CChar> {
    let escaped = message
        .replacingOccurrences(of: "\\", with: "\\\\")
        .replacingOccurrences(of: "\"", with: "\\\"")
        .replacingOccurrences(of: "\n", with: "\\n")
    return allocateCString("{\"error\":\"\(escaped)\"}")
}

/// Map an `MLFeatureType` to a human-readable string.
private func featureTypeName(_ type: MLFeatureType) -> String {
    switch type {
    case .invalid:      return "invalid"
    case .int64:        return "int64"
    case .double:       return "float64"
    case .string:       return "string"
    case .image:        return "image"
    case .multiArray:   return "multiArray"
    case .dictionary:   return "dictionary"
    case .sequence:     return "sequence"
    case .state:        return "state"
    @unknown default:   return "unknown"
    }
}

/// Extract shape information from a multi-array constraint if present.
private func shapeArray(_ desc: MLFeatureDescription) -> [Int]? {
    if let constraint = desc.multiArrayConstraint {
        return constraint.shape.map { $0.intValue }
    }
    return nil
}

/// Serialize a feature description into a dictionary suitable for JSON encoding.
private func portDict(_ name: String, _ desc: MLFeatureDescription) -> [String: Any] {
    var d: [String: Any] = [
        "name": name,
        "port_type": featureTypeName(desc.type),
    ]
    if let shape = shapeArray(desc) {
        d["shape"] = shape
    }
    if let imageConstraint = desc.imageConstraint {
        d["shape"] = [
            Int(imageConstraint.pixelsHigh),
            Int(imageConstraint.pixelsWide),
        ]
    }
    return d
}

/// Determine model type by inspecting feature descriptions.
private func classifyModelType(inputs: [String: MLFeatureDescription],
                               outputs: [String: MLFeatureDescription]) -> String {
    var hasImage = false
    var hasText = false
    var hasAudio = false

    for (_, desc) in inputs {
        switch desc.type {
        case .image:
            hasImage = true
        case .string:
            hasText = true
        case .multiArray:
            // Heuristic: large 1-D arrays with names containing "audio" or "mel" suggest audio.
            let name = desc.multiArrayConstraint?.shape.description.lowercased() ?? ""
            if name.contains("audio") || name.contains("mel") {
                hasAudio = true
            }
            // Multi-array can also be text embeddings; use name hints from the key later.
        default:
            break
        }
    }

    // Check input key names for extra hints.
    for key in inputs.keys {
        let lower = key.lowercased()
        if lower.contains("image") || lower.contains("pixel") {
            hasImage = true
        }
        if lower.contains("text") || lower.contains("prompt") || lower.contains("input_ids") || lower.contains("token") {
            hasText = true
        }
        if lower.contains("audio") || lower.contains("mel") || lower.contains("waveform") {
            hasAudio = true
        }
    }

    if hasImage && hasText { return "Multimodal" }
    if hasImage           { return "Vision" }
    if hasAudio           { return "Audio" }
    if hasText            { return "Text" }
    return "Unknown"
}


// MARK: - Public C API

/// Load a CoreML model from a file path.
///
/// Accepts `.mlmodelc` (compiled), `.mlmodel` (compiled on disk), and `.mlpackage` (source,
/// will be compiled in a temporary directory on first load).
///
/// Returns an opaque handle on success or `nil` on failure. The caller must eventually call
/// `coreml_unload_model` to release the handle.
@_cdecl("coreml_load_model")
public func coreml_load_model(path: UnsafePointer<CChar>) -> UnsafeMutableRawPointer? {
    let pathStr = String(cString: path)
    let sourceURL = URL(fileURLWithPath: pathStr)

    do {
        let compiledURL: URL

        if pathStr.hasSuffix(".mlmodelc") {
            // Already compiled.
            compiledURL = sourceURL
        } else if pathStr.hasSuffix(".mlpackage") || pathStr.hasSuffix(".mlmodel") {
            // Compile from source. `compileModel(at:)` writes to a temporary directory.
            compiledURL = try MLModel.compileModel(at: sourceURL)
        } else {
            return nil
        }

        let config = MLModelConfiguration()
        config.computeUnits = .all

        let model = try MLModel(contentsOf: compiledURL, configuration: config)
        let handle = ModelHandle(model: model, sourceURL: sourceURL)

        // Transfer ownership to the caller via an unmanaged pointer.
        return Unmanaged.passRetained(handle).toOpaque()
    } catch {
        // Cannot return an error string here (return type is a raw pointer), so return nil.
        // Callers should check for nil and treat it as a load failure.
        return nil
    }
}

/// Release a previously loaded model handle.
@_cdecl("coreml_unload_model")
public func coreml_unload_model(handle: UnsafeMutableRawPointer) {
    // Balance the `passRetained` from load.
    Unmanaged<ModelHandle>.fromOpaque(handle).release()
}

/// Retrieve model metadata as a JSON string.
///
/// The returned JSON has the shape:
/// ```json
/// {
///   "description": "...",
///   "author": "...",
///   "model_type": "Text",
///   "input_schema": [{"name": "...", "port_type": "...", "shape": [...]}],
///   "output_schema": [...]
/// }
/// ```
///
/// The caller must free the returned pointer with `coreml_free_string`.
@_cdecl("coreml_get_metadata")
public func coreml_get_metadata(handle: UnsafeMutableRawPointer) -> UnsafePointer<CChar>? {
    let wrapper = Unmanaged<ModelHandle>.fromOpaque(handle).takeUnretainedValue()
    let desc = wrapper.model.modelDescription

    // Build input/output schema arrays.
    var inputs: [[String: Any]] = []
    for (name, feature) in desc.inputDescriptionsByName {
        inputs.append(portDict(name, feature))
    }

    var outputs: [[String: Any]] = []
    for (name, feature) in desc.outputDescriptionsByName {
        outputs.append(portDict(name, feature))
    }

    // Sort for deterministic output.
    inputs.sort { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }
    outputs.sort { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }

    let modelType = classifyModelType(
        inputs: desc.inputDescriptionsByName,
        outputs: desc.outputDescriptionsByName
    )

    var meta: [String: Any] = [
        "model_type": modelType,
        "input_schema": inputs,
        "output_schema": outputs,
    ]

    if let d = desc.metadata[MLModelMetadataKey.description] as? String, !d.isEmpty {
        meta["description"] = d
    }
    if let a = desc.metadata[MLModelMetadataKey.author] as? String, !a.isEmpty {
        meta["author"] = a
    }

    do {
        let data = try JSONSerialization.data(withJSONObject: meta, options: [.sortedKeys])
        if let json = String(data: data, encoding: .utf8) {
            return allocateCString(json)
        }
        return errorJSON("Failed to encode metadata as UTF-8")
    } catch {
        return errorJSON("JSON serialization failed: \(error.localizedDescription)")
    }
}

/// Run text-based inference.
///
/// `input_json` must be a JSON object mapping feature names to values, e.g.:
/// ```json
/// {"prompt": "Hello, world!"}
/// ```
///
/// Returns a JSON string with the model output or an error object.
/// The caller must free the returned pointer with `coreml_free_string`.
@_cdecl("coreml_predict_text")
public func coreml_predict_text(handle: UnsafeMutableRawPointer,
                                input_json: UnsafePointer<CChar>) -> UnsafePointer<CChar>? {
    let wrapper = Unmanaged<ModelHandle>.fromOpaque(handle).takeUnretainedValue()
    let jsonStr = String(cString: input_json)

    guard let jsonData = jsonStr.data(using: .utf8),
          let dict = try? JSONSerialization.jsonObject(with: jsonData) as? [String: Any] else {
        return errorJSON("Invalid JSON input")
    }

    do {
        let provider = try MLDictionaryFeatureProvider(dictionary: dict)
        let result = try wrapper.model.prediction(from: provider)
        let output = featureProviderToDict(result)

        let data = try JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
        if let json = String(data: data, encoding: .utf8) {
            return allocateCString(json)
        }
        return errorJSON("Failed to encode prediction result as UTF-8")
    } catch {
        return errorJSON("Prediction failed: \(error.localizedDescription)")
    }
}

/// Run image-based inference.
///
/// `image_data` / `image_len` represent raw image bytes (JPEG, PNG, etc.).
/// `prompt` is an optional text prompt for multimodal models (may be NULL).
///
/// Returns a JSON string with the model output or an error object.
/// The caller must free the returned pointer with `coreml_free_string`.
@_cdecl("coreml_predict_image")
public func coreml_predict_image(handle: UnsafeMutableRawPointer,
                                 image_data: UnsafePointer<UInt8>,
                                 image_len: Int,
                                 prompt: UnsafePointer<CChar>?) -> UnsafePointer<CChar>? {
    let wrapper = Unmanaged<ModelHandle>.fromOpaque(handle).takeUnretainedValue()
    let desc = wrapper.model.modelDescription

    // Find the image input feature.
    guard let (imageName, imageFeature) = desc.inputDescriptionsByName.first(where: { $0.value.type == .image }) else {
        return errorJSON("Model has no image input feature")
    }

    // Decode image bytes into a CVPixelBuffer.
    let data = Data(bytes: image_data, count: image_len)
    guard let nsImage = NSImage(data: data) else {
        return errorJSON("Failed to decode image data")
    }

    guard let cgImage = nsImage.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
        return errorJSON("Failed to convert image to CGImage")
    }

    // Determine target size from model constraint, or use the image's natural size.
    let targetWidth: Int
    let targetHeight: Int
    if let constraint = imageFeature.imageConstraint {
        targetWidth = Int(constraint.pixelsWide)
        targetHeight = Int(constraint.pixelsHigh)
    } else {
        targetWidth = cgImage.width
        targetHeight = cgImage.height
    }

    guard let pixelBuffer = cgImageToPixelBuffer(cgImage,
                                                  width: targetWidth,
                                                  height: targetHeight) else {
        return errorJSON("Failed to create pixel buffer from image")
    }

    do {
        var inputDict: [String: Any] = [
            imageName: pixelBuffer,
        ]

        // If there is a text/prompt input, fill it in.
        if let promptPtr = prompt {
            let promptStr = String(cString: promptPtr)
            // Find string-typed inputs that are not the image feature.
            for (name, feature) in desc.inputDescriptionsByName {
                if feature.type == .string && name != imageName {
                    inputDict[name] = promptStr
                    break
                }
            }
        }

        let provider = try MLDictionaryFeatureProvider(dictionary: inputDict)
        let result = try wrapper.model.prediction(from: provider)
        let output = featureProviderToDict(result)

        let data = try JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
        if let json = String(data: data, encoding: .utf8) {
            return allocateCString(json)
        }
        return errorJSON("Failed to encode prediction result as UTF-8")
    } catch {
        return errorJSON("Prediction failed: \(error.localizedDescription)")
    }
}

/// Free a string previously returned by any `coreml_*` function.
@_cdecl("coreml_free_string")
public func coreml_free_string(ptr: UnsafePointer<CChar>) {
    ptr.deallocate()
}


// MARK: - Internal Utilities

/// Convert an `MLFeatureProvider` (prediction result) into a JSON-serializable dictionary.
private func featureProviderToDict(_ provider: MLFeatureProvider) -> [String: Any] {
    var dict: [String: Any] = [:]
    for name in provider.featureNames {
        guard let value = provider.featureValue(for: name) else { continue }
        dict[name] = featureValueToAny(value)
    }
    return dict
}

/// Convert a single `MLFeatureValue` to a JSON-compatible Swift value.
private func featureValueToAny(_ value: MLFeatureValue) -> Any {
    switch value.type {
    case .string:
        return value.stringValue
    case .int64:
        return value.int64Value
    case .double:
        return value.doubleValue
    case .multiArray:
        if let ma = value.multiArrayValue {
            return multiArrayToArray(ma)
        }
        return NSNull()
    case .dictionary:
        if let d = value.dictionaryValue as? [String: NSNumber] {
            var out: [String: Any] = [:]
            for (k, v) in d {
                out[k] = v.doubleValue
            }
            return out
        }
        if let d = value.dictionaryValue as? [NSNumber: NSNumber] {
            // Convert numeric keys to strings for JSON compatibility.
            var out: [String: Any] = [:]
            for (k, v) in d {
                out[k.stringValue] = v.doubleValue
            }
            return out
        }
        return value.dictionaryValue as Any
    case .sequence:
        if let seq = value.sequenceValue {
            var arr: [Any] = []
            for i in 0..<seq.int64Values.count {
                arr.append(seq.int64Values[i])
            }
            if arr.isEmpty {
                for i in 0..<seq.stringValues.count {
                    arr.append(seq.stringValues[i])
                }
            }
            return arr
        }
        return NSNull()
    case .image:
        // Images in output are not directly JSON-serializable; return a placeholder.
        return "[image output]"
    default:
        return NSNull()
    }
}

/// Flatten an `MLMultiArray` into a nested Swift array for JSON serialization.
private func multiArrayToArray(_ ma: MLMultiArray) -> Any {
    let shape = ma.shape.map { $0.intValue }

    // For 1-D arrays, return a flat array.
    if shape.count == 1 {
        var arr: [Any] = []
        arr.reserveCapacity(shape[0])
        for i in 0..<shape[0] {
            arr.append(ma[[NSNumber(value: i)]].doubleValue)
        }
        return arr
    }

    // For higher-dimensional arrays, return shape + flattened data to keep the response compact.
    let total = ma.count
    var flat: [Double] = []
    flat.reserveCapacity(min(total, 10000)) // Cap to avoid enormous outputs.
    let cap = min(total, 10000)
    for i in 0..<cap {
        flat.append(ma[i].doubleValue)
    }

    var result: [String: Any] = [
        "shape": shape,
        "data": flat,
    ]
    if total > cap {
        result["truncated"] = true
        result["total_elements"] = total
    }
    return result
}

/// Convert a CGImage to a CVPixelBuffer at the specified dimensions.
private func cgImageToPixelBuffer(_ image: CGImage, width: Int, height: Int) -> CVPixelBuffer? {
    let attrs: [CFString: Any] = [
        kCVPixelBufferCGImageCompatibilityKey: true,
        kCVPixelBufferCGBitmapContextCompatibilityKey: true,
    ]

    var pixelBuffer: CVPixelBuffer?
    let status = CVPixelBufferCreate(
        kCFAllocatorDefault,
        width,
        height,
        kCVPixelFormatType_32ARGB,
        attrs as CFDictionary,
        &pixelBuffer
    )

    guard status == kCVReturnSuccess, let buffer = pixelBuffer else {
        return nil
    }

    CVPixelBufferLockBaseAddress(buffer, [])
    defer { CVPixelBufferUnlockBaseAddress(buffer, []) }

    guard let context = CGContext(
        data: CVPixelBufferGetBaseAddress(buffer),
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: CVPixelBufferGetBytesPerRow(buffer),
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue
    ) else {
        return nil
    }

    context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    return buffer
}
