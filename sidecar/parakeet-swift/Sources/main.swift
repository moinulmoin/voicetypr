import Foundation
import FluidAudio

// Helper function to log to stderr (so it doesn't interfere with JSON on stdout)
func log(_ message: String) {
    fputs("\(message)\n", stderr)
    fflush(stderr)
}

// JSON message structures for communication with Tauri
struct TranscribeRequest: Codable {
    let type: String
    let audio_path: String
}

struct TranscriptionResponse: Codable {
    let type: String = "transcription"
    let text: String
    let segments: [Segment]
    let language: String?
    let duration: Float?

    init(text: String, segments: [Segment] = [], language: String? = nil, duration: Float? = nil) {
        self.text = text
        self.segments = segments
        self.language = language
        self.duration = duration
    }
}

struct Segment: Codable {
    let text: String
}

struct StatusResponse: Codable {
    let type: String = "status"
    let loadedModel: String? // camelCase to match Rust's expectation
    let modelPath: String? = nil
    let precision: String? = nil
    let attention: String? = nil
    
    // No CodingKeys needed - Swift's camelCase matches Rust's camelCase
}

struct ErrorResponse: Codable {
    let type: String = "error"
    let code: String
    let message: String
    let details: [String: String]? = nil  // Optional details field to match Rust
}

// Global ASR manager
var asrManager: AsrManager?
var isModelLoaded = false
var isModelDownloaded = false

@main
struct ParakeetSidecar {
    static func main() async {
        // Set up JSON encoder
        // IMPORTANT: Do NOT use .prettyPrinted - Rust parses line-by-line
        // Multi-line JSON will cause "EOF while parsing" errors
        let encoder = JSONEncoder()

        // Process command line arguments or stdin
        if CommandLine.arguments.count > 1 {
            // Direct file mode for testing
            let audioPath = CommandLine.arguments[1]
            await transcribeFile(audioPath, language: nil, translateToEnglish: false, encoder: encoder)
        } else {
            // JSON communication mode for Tauri
            await runEventLoop(encoder: encoder)
        }
    }

    static func runEventLoop(encoder: JSONEncoder) async {
        while let line = readLine() {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { continue }

            do {
                guard let data = trimmed.data(using: .utf8) else {
                    sendError("invalid_encoding", message: "Failed to parse command payload", encoder: encoder)
                    continue
                }

                guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    sendError("invalid_payload", message: "Command payload must be a JSON object", encoder: encoder)
                    continue
                }

                switch json["type"] as? String {
                case "load_model", "download_model":
                    // Handle both load_model and download_model commands
                    // This allows the UI's Download button to trigger model download
                    let modelId = json["model_id"] as? String
                    await loadModel(modelId: modelId, encoder: encoder)

                case "unload_model":
                    unloadModel()
                    sendResponse(StatusResponse(loadedModel: nil), encoder: encoder)

                case "delete_model":
                    // Delete the actual model files from FluidAudio cache
                    deleteModelFiles()
                    unloadModel()  // Also unload from memory
                    sendResponse(StatusResponse(loadedModel: nil), encoder: encoder)

                case "transcribe":
                    if let audioPath = json["audio_path"] as? String {
                        // Extract optional parameters from Rust backend
                        let language = json["language"] as? String
                        let translateToEnglish = json["translate_to_english"] as? Bool ?? false
                        await transcribeFile(audioPath, language: language, translateToEnglish: translateToEnglish, encoder: encoder)
                    } else {
                        sendError("missing_audio_path", message: "audio_path is required", encoder: encoder)
                    }

                case "status":
                    sendResponse(StatusResponse(
                        loadedModel: isModelLoaded ? "parakeet-tdt-0.6b-v3" : nil
                    ), encoder: encoder)

                case "shutdown":
                    unloadModel()
                    exit(0)

                default:
                    sendError("unknown_command", message: "Unknown command type", encoder: encoder)
                }
            } catch {
                sendError("parse_error", message: "Failed to parse JSON: \(error)", encoder: encoder)
            }
        }
    }

    static func loadModel(modelId: String? = nil, encoder: JSONEncoder) async {
        // Use provided model_id or default to parakeet-tdt-0.6b-v3
        let actualModelId = modelId ?? "parakeet-tdt-0.6b-v3"

        // If already loaded with same model, just return success
        if isModelLoaded {
            log("⚡ Model already loaded: \(actualModelId)")
            sendResponse(StatusResponse(loadedModel: actualModelId), encoder: encoder)
            return
        }

        do {
            // Download models NOW when user clicks Download button
            // This ensures user has control over when download happens
            log("📥 Starting Parakeet model download via FluidAudio...")
            log("📦 Model: \(actualModelId)")
            log("🌐 This will download ~500MB from FluidAudio servers...")
            log("⏳ Please wait, this may take 2-5 minutes depending on your connection...")
            
            let models = try await AsrModels.downloadAndLoad()
            
            log("✅ Model downloaded successfully!")
            log("🔧 Initializing ASR manager...")
            isModelDownloaded = true

            // Initialize ASR manager with downloaded models
            let manager = AsrManager(config: .default)
            try await manager.initialize(models: models)
            asrManager = manager

            log("✅ ASR manager initialized, model ready for use!")
            isModelLoaded = true
            sendResponse(StatusResponse(loadedModel: actualModelId), encoder: encoder)
        } catch {
            log("❌ Failed to load model: \(error)")
            sendError("model_load_error", message: "Failed to load model: \(error)", encoder: encoder)
        }
    }

    static func unloadModel() {
        asrManager?.cleanup()
        asrManager = nil
        isModelLoaded = false
    }

    static func deleteModelFiles() {
        // FluidAudio stores models in ~/Library/Application Support/
        // We need to delete the actual model files
        let fileManager = FileManager.default

        // Possible locations where FluidAudio might store models
        let appSupportPaths = [
            fileManager.homeDirectoryForCurrentUser
                .appendingPathComponent("Library/Application Support/FluidAudio"),
            fileManager.homeDirectoryForCurrentUser
                .appendingPathComponent("Library/Application Support/parakeet-tdt-0.6b-v3-coreml"),
            fileManager.homeDirectoryForCurrentUser
                .appendingPathComponent("Library/Caches/FluidAudio")
        ]

        for path in appSupportPaths {
            if fileManager.fileExists(atPath: path.path) {
                do {
                    try fileManager.removeItem(at: path)
                    log("🗑️  Deleted model files at: \(path.path)")
                } catch {
                    log("⚠️  Failed to delete model files at \(path.path): \(error)")
                }
            }
        }

        // Mark as not downloaded after deletion
        isModelDownloaded = false
    }

    static func transcribeFile(_ audioPath: String, language: String? = nil, translateToEnglish: Bool = false, encoder: JSONEncoder) async {
        // Check if model is loaded - DO NOT auto-download
        guard isModelLoaded else {
            sendError("model_not_loaded", message: "Parakeet model not loaded. Please download it first from Settings.", encoder: encoder)
            return
        }

        let fileURL = URL(fileURLWithPath: audioPath)

        // Check if file exists
        guard FileManager.default.fileExists(atPath: audioPath) else {
            sendError("file_not_found", message: "Audio file not found: \(audioPath)", encoder: encoder)
            return
        }

        guard let manager = asrManager else {
            sendError("model_not_loaded", message: "Parakeet engine is not initialized", encoder: encoder)
            return
        }

        do {
            // Transcribe the audio file (returns ASRResult)
            let result = try await manager.transcribe(fileURL)

            // Send transcription response
            let response = TranscriptionResponse(
                text: result.text,
                segments: [],  // FluidAudio doesn't provide segments
                language: language,  // Pass through the language if provided
                duration: Float(result.duration)
            )
            sendResponse(response, encoder: encoder)
        } catch {
            // Send error response instead of transcription with error
            sendError("transcription_failed", message: "Transcription failed: \(error)", encoder: encoder)
        }
    }

    static func sendResponse<T: Codable>(_ response: T, encoder: JSONEncoder) {
        do {
            let data = try encoder.encode(response)
            if let jsonString = String(data: data, encoding: .utf8) {
                print(jsonString)
                fflush(stdout)
            }
        } catch {
            print("{\"type\":\"error\",\"code\":\"serialization_error\",\"message\":\"Failed to serialize response\"}")
            fflush(stdout)
        }
    }

    static func sendError(_ code: String, message: String, encoder: JSONEncoder) {
        sendResponse(ErrorResponse(code: code, message: message), encoder: encoder)
    }
}

