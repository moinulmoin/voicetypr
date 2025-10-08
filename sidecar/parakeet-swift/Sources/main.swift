import Foundation
import FluidAudio

// Helper function to log to stderr (so it doesn't interfere with JSON on stdout)
func log(_ message: String) {
    fputs("\(message)\n", stderr)
    fflush(stderr)
}

// JSON message structures for communication with Tauri
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
    let loadedModel: String?
    let modelVersion: String?
    let modelPath: String? = nil
    let precision: String? = nil
    let attention: String? = nil
}

struct ErrorResponse: Codable {
    let type: String = "error"
    let code: String
    let message: String
    let details: [String: String]? = nil  // Optional details field to match Rust
}

enum SupportedModelVersion: String, CaseIterable {
    case v2
    case v3

    var asrVersion: AsrModelVersion {
        switch self {
        case .v2: return .v2
        case .v3: return .v3
        }
    }

    var modelIdentifier: String {
        switch self {
        case .v2: return "parakeet-tdt-0.6b-v2"
        case .v3: return "parakeet-tdt-0.6b-v3"
        }
    }

    var repoFolderName: String {
        "\(modelIdentifier)-coreml"
    }
}

// Global ASR manager state
var asrManager: AsrManager?
var isModelLoaded = false
var loadedModelVersion: SupportedModelVersion?
var downloadedVersions = Set<SupportedModelVersion>()

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
            await loadModel(version: .v3, forceDownload: true, emitStatus: false, encoder: encoder)
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
                    let version = parseModelVersion(json["model_version"], fallbackModelId: json["model_id"] as? String)
                    let forceDownload: Bool
                    if let explicit = json["force_download"] as? Bool {
                        forceDownload = explicit
                    } else {
                        forceDownload = (json["type"] as? String) == "download_model"
                    }
                    await loadModel(version: version, forceDownload: forceDownload, encoder: encoder)

                case "unload_model":
                    unloadModel()
                    sendResponse(StatusResponse(loadedModel: nil, modelVersion: nil), encoder: encoder)

                case "delete_model":
                    let version = parseModelVersion(json["model_version"], fallbackModelId: json["model_id"] as? String)
                    deleteModelFiles(for: version)
                    if loadedModelVersion == version {
                        unloadModel()
                    }
                    sendResponse(StatusResponse(loadedModel: loadedModelVersion?.modelIdentifier, modelVersion: loadedModelVersion?.rawValue), encoder: encoder)

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
                    sendResponse(
                        StatusResponse(
                            loadedModel: loadedModelVersion?.modelIdentifier,
                            modelVersion: loadedModelVersion?.rawValue
                        ),
                        encoder: encoder
                    )

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

    static func loadModel(version: SupportedModelVersion = .v3, forceDownload: Bool = false, emitStatus: Bool = true, encoder: JSONEncoder) async {
        if isModelLoaded, let loadedVersion = loadedModelVersion, loadedVersion == version {
            log("⚡ Model already loaded: \(loadedVersion.modelIdentifier)")
            if emitStatus {
                sendResponse(StatusResponse(loadedModel: loadedVersion.modelIdentifier, modelVersion: loadedVersion.rawValue), encoder: encoder)
            }
            return
        }

        do {
            let models: AsrModels

            if forceDownload {
                log("📥 Force-downloading Parakeet \(version.rawValue.uppercased()) via FluidAudio...")
                log("🌐 This will download ~500MB. Please wait...")
                models = try await AsrModels.downloadAndLoad(version: version.asrVersion)
                downloadedVersions.insert(version)
            } else {
                log("🔍 Attempting to load Parakeet \(version.rawValue.uppercased()) from cache...")
                do {
                    models = try await AsrModels.loadFromCache(version: version.asrVersion)
                    downloadedVersions.insert(version)
                    log("✅ Loaded Parakeet \(version.rawValue.uppercased()) from cache")
                } catch {
                    log("❌ Failed to load from cache: \(error)")
                    sendError("model_not_downloaded", message: "Parakeet \(version.rawValue.uppercased()) is not downloaded. Please download it first.", encoder: encoder)
                    return
                }
            }

            let manager = AsrManager(config: .default)
            try await manager.initialize(models: models)
            asrManager = manager

            isModelLoaded = true
            loadedModelVersion = version
            if emitStatus {
                sendResponse(StatusResponse(loadedModel: version.modelIdentifier, modelVersion: version.rawValue), encoder: encoder)
            }
        } catch {
            log("❌ Failed to load model: \(error)")
            sendError("model_load_error", message: "Failed to load model: \(error)", encoder: encoder)
        }
    }

    static func unloadModel() {
        asrManager?.cleanup()
        asrManager = nil
        isModelLoaded = false
        loadedModelVersion = nil
    }

    static func deleteModelFiles(for version: SupportedModelVersion) {
        let fileManager = FileManager.default

        let home = fileManager.homeDirectoryForCurrentUser
        let repoFolder = version.repoFolderName

        let targets: [URL] = [
            home
                .appendingPathComponent("Library/Application Support/FluidAudio/Models", isDirectory: true)
                .appendingPathComponent(repoFolder, isDirectory: true),
            home
                .appendingPathComponent("Library/Application Support", isDirectory: true)
                .appendingPathComponent(repoFolder, isDirectory: true),
            home
                .appendingPathComponent("Library/Caches/FluidAudio", isDirectory: true)
                .appendingPathComponent(repoFolder, isDirectory: true)
        ]

        for path in targets {
            if fileManager.fileExists(atPath: path.path) {
                do {
                    try fileManager.removeItem(at: path)
                    log("🗑️  Deleted model files at: \(path.path)")
                } catch {
                    log("⚠️  Failed to delete model files at \(path.path): \(error)")
                }
            }
        }

        downloadedVersions.remove(version)
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
                segments: [],
                language: language,
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

    static func parseModelVersion(_ value: Any?) -> SupportedModelVersion {
        if let str = (value as? String)?.lowercased(), str == "v2" {
            return .v2
        }
        return .v3
    }

    static func parseModelVersion(_ value: Any?, fallbackModelId: String?) -> SupportedModelVersion {
        if let value = value {
            return parseModelVersion(value)
        }

        if let modelId = fallbackModelId?.lowercased(), modelId.contains("-v2") {
            return .v2
        }

        return .v3
    }
}

