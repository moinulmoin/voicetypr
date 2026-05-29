import Foundation
import FluidAudio

// Helper function to log to stderr (so it doesn't interfere with JSON on stdout)
func log(_ message: String) {
    fputs("\(message)\n", stderr)
    fflush(stderr)
}

// Get system architecture info
func getArchitectureInfo() -> String {
    #if arch(arm64)
    return "arm64 (Apple Silicon)"
    #elseif arch(x86_64)
    return "x86_64 (Intel)"
    #else
    return "unknown"
    #endif
}

// Log system information for debugging
func logSystemInfo() {
    log("🦜 Parakeet sidecar started")
    log("   Architecture: \(getArchitectureInfo())")
    log("   macOS: \(ProcessInfo.processInfo.operatingSystemVersionString)")
    log("   PID: \(ProcessInfo.processInfo.processIdentifier)")
}

// JSON message structures for communication with Tauri
struct TranscriptionResponse: Encodable {
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

struct DiarizationResponse: Encodable {
    let type: String = "diarization"
    let segments: [SpeakerSegment]
}

struct SpeakerSegment: Encodable {
    let speakerId: String
    let start: Float
    let end: Float
}

struct Segment: Encodable {
    let text: String
}

struct StatusResponse: Encodable {
    let type: String = "status"
    let loadedModel: String?
    let modelVersion: String?
    let modelPath: String? = nil
    let precision: String? = nil
    let attention: String? = nil
}

struct ProgressResponse: Encodable {
    let type: String = "progress"
    let progress: Double
    let phase: String
}

struct ErrorResponse: Encodable {
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
        modelIdentifier
    }
}

// Global ASR manager state
@MainActor var asrManager: AsrManager?
@MainActor var isModelLoaded = false
@MainActor var loadedModelVersion: SupportedModelVersion?
@MainActor var downloadedVersions = Set<SupportedModelVersion>()

@MainActor
@main
struct ParakeetSidecar {
    static func main() async {
        logSystemInfo()

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
                    guard let version = parseModelVersion(json["model_version"]) else {
                        sendError("invalid_model_version", message: "model_version must be \"v2\" or \"v3\"", encoder: encoder)
                        continue
                    }
                    let forceDownload: Bool
                    if let explicit = json["force_download"] as? Bool {
                        forceDownload = explicit
                    } else {
                        forceDownload = (json["type"] as? String) == "download_model"
                    }
                    await loadModel(version: version, forceDownload: forceDownload, encoder: encoder)

                case "unload_model":
                    await unloadModel()
                    sendResponse(StatusResponse(loadedModel: nil, modelVersion: nil), encoder: encoder)

                case "delete_model":
                    guard let version = parseModelVersion(json["model_version"]) else {
                        sendError("invalid_model_version", message: "model_version must be \"v2\" or \"v3\"", encoder: encoder)
                        continue
                    }
                    deleteModelFiles(for: version)
                    if loadedModelVersion == version {
                        await unloadModel()
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

                case "diarize":
                    if let audioPath = json["audio_path"] as? String {
                        await diarizeFile(audioPath, encoder: encoder)
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
                    await unloadModel()
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
        log("───────────────────────────────────────────────────────")
        log("🔄 LOAD MODEL REQUEST")
        log("───────────────────────────────────────────────────────")
        log("📦 Requested version: \(version.rawValue.uppercased()) (\(version.modelIdentifier))")
        log("📁 Repo folder: \(version.repoFolderName)")
        log("🔄 Force download: \(forceDownload)")
        log("📐 Running on: \(getArchitectureInfo())")

        // Check expected cache path
        let home = FileManager.default.homeDirectoryForCurrentUser
        let expectedPath = home
            .appendingPathComponent("Library/Application Support/FluidAudio/Models")
            .appendingPathComponent(version.repoFolderName)
        log("📍 Expected cache path: \(expectedPath.path)")
        log("📂 Path exists: \(FileManager.default.fileExists(atPath: expectedPath.path))")

        if FileManager.default.fileExists(atPath: expectedPath.path) {
            if let contents = try? FileManager.default.contentsOfDirectory(atPath: expectedPath.path) {
                log("📄 Cache contents: \(contents.joined(separator: ", "))")
            }
        }

        if isModelLoaded, let loadedVersion = loadedModelVersion, loadedVersion == version {
            log("⚡ Model already loaded: \(loadedVersion.modelIdentifier)")
            if emitStatus {
                sendResponse(StatusResponse(loadedModel: loadedVersion.modelIdentifier, modelVersion: loadedVersion.rawValue), encoder: encoder)
            }
            return
        }

        do {
            let models: AsrModels
            let progressHandler: DownloadUtils.ProgressHandler = { progress in
                sendProgress(progress, encoder: encoder)
            }

            if forceDownload {
                log("📥 Force-downloading Parakeet \(version.rawValue.uppercased()) via FluidAudio...")
                log("🌐 This will download ~500MB. Please wait...")
                models = try await AsrModels.downloadAndLoad(version: version.asrVersion, progressHandler: progressHandler)
                downloadedVersions.insert(version)
                log("✅ Download complete for \(version.rawValue.uppercased())")
            } else {
                log("🔍 Attempting to load Parakeet \(version.rawValue.uppercased()) from cache...")
                do {
                    models = try await AsrModels.loadFromCache(version: version.asrVersion, progressHandler: progressHandler)
                    downloadedVersions.insert(version)
                    log("✅ Loaded Parakeet \(version.rawValue.uppercased()) from cache")
                } catch {
                    log("❌ Failed to load \(version.rawValue.uppercased()) from cache")
                    log("❌ Error type: \(type(of: error))")
                    log("❌ Error details: \(error)")
                    log("❌ Localized: \(error.localizedDescription)")
                    sendError("model_not_downloaded", message: "Parakeet \(version.rawValue.uppercased()) is not downloaded. Please download it first. Error: \(error.localizedDescription)", encoder: encoder)
                    return
                }
            }

            log("🔧 Initializing AsrManager...")
            let manager = AsrManager(config: .default)
            log("🔧 Calling manager.loadModels(_:)...")
            try await manager.loadModels(models)
            log("✅ AsrManager initialized successfully")
            asrManager = manager

            isModelLoaded = true
            loadedModelVersion = version
            log("✅ Model load complete: \(version.modelIdentifier)")
            if emitStatus {
                sendResponse(StatusResponse(loadedModel: version.modelIdentifier, modelVersion: version.rawValue), encoder: encoder)
            }
        } catch {
            log("❌ FATAL: Failed to load model \(version.rawValue.uppercased())")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            sendError("model_load_error", message: "Failed to load model: \(error.localizedDescription)", encoder: encoder)
        }
        log("───────────────────────────────────────────────────────")
    }

    static func unloadModel() async {
        await asrManager?.cleanup()
        asrManager = nil
        isModelLoaded = false
        loadedModelVersion = nil
    }

    static func deleteModelFiles(for version: SupportedModelVersion) {
        let fileManager = FileManager.default

        let home = fileManager.homeDirectoryForCurrentUser
        let targets: [URL] = [
            home
                .appendingPathComponent("Library/Application Support/FluidAudio/Models", isDirectory: true)
                .appendingPathComponent(version.repoFolderName, isDirectory: true),
            home
                .appendingPathComponent("Library/Application Support", isDirectory: true)
                .appendingPathComponent(version.repoFolderName, isDirectory: true),
            home
                .appendingPathComponent("Library/Caches/FluidAudio", isDirectory: true)
                .appendingPathComponent(version.repoFolderName, isDirectory: true)
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
        log("───────────────────────────────────────────────────────")
        log("🎤 TRANSCRIBE REQUEST")
        log("───────────────────────────────────────────────────────")
        log("📄 Audio path: \(audioPath)")
        log("🌐 Language: \(language ?? "auto-detect")")
        log("🔄 Translate to English: \(translateToEnglish)")
        log("📦 Loaded model: \(loadedModelVersion?.modelIdentifier ?? "none")")
        log("📐 Running on: \(getArchitectureInfo())")

        // Check if model is loaded - DO NOT auto-download
        guard isModelLoaded else {
            log("❌ No model loaded!")
            sendError("model_not_loaded", message: "Parakeet model not loaded. Please download it first from Settings.", encoder: encoder)
            return
        }

        let fileURL = URL(fileURLWithPath: audioPath)

        // Check if file exists
        guard FileManager.default.fileExists(atPath: audioPath) else {
            log("❌ Audio file not found: \(audioPath)")
            sendError("file_not_found", message: "Audio file not found: \(audioPath)", encoder: encoder)
            return
        }

        // Log file info
        if let attrs = try? FileManager.default.attributesOfItem(atPath: audioPath) {
            let size = attrs[.size] as? Int64 ?? 0
            log("📊 File size: \(size) bytes (\(size / 1024) KB)")
        }

        guard let manager = asrManager else {
            log("❌ AsrManager is nil even though isModelLoaded=true!")
            sendError("model_not_loaded", message: "Parakeet engine is not initialized", encoder: encoder)
            return
        }

        do {
            log("🎙️ Starting transcription...")
            let startTime = Date()

            // Transcribe the audio file (returns ASRResult)
            var decoderState = TdtDecoderState.make(decoderLayers: await manager.decoderLayerCount)
            let result = try await manager.transcribe(fileURL, decoderState: &decoderState)

            let elapsed = Date().timeIntervalSince(startTime)
            log("✅ Transcription complete in \(String(format: "%.2f", elapsed))s")
            log("📝 Result text length: \(result.text.count) chars")
            log("⏱️ Audio duration: \(result.duration)s")

            // Send transcription response
            let response = TranscriptionResponse(
                text: result.text,
                segments: [],
                language: language,
                duration: Float(result.duration)
            )
            sendResponse(response, encoder: encoder)
        } catch {
            log("❌ TRANSCRIPTION FAILED")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            // Send error response instead of transcription with error
            sendError("transcription_failed", message: "Transcription failed: \(error.localizedDescription)", encoder: encoder)
        }
        log("───────────────────────────────────────────────────────")
    }

    nonisolated static func diarizeFile(_ audioPath: String, encoder: JSONEncoder) async {
        log("───────────────────────────────────────────────────────")
        log("👥 DIARIZATION REQUEST")
        log("───────────────────────────────────────────────────────")
        log("📄 Audio path: \(audioPath)")

        guard FileManager.default.fileExists(atPath: audioPath) else {
            log("❌ Audio file not found: \(audioPath)")
            sendError("file_not_found", message: "Audio file not found: \(audioPath)", encoder: encoder)
            return
        }

        do {
            let manager = OfflineDiarizerManager()
            try await manager.prepareModels()
            let result = try await manager.process(URL(fileURLWithPath: audioPath))
            let segments = result.segments.map { segment in
                SpeakerSegment(
                    speakerId: segment.speakerId,
                    start: segment.startTimeSeconds,
                    end: segment.endTimeSeconds
                )
            }

            sendResponse(DiarizationResponse(segments: segments), encoder: encoder)
        } catch {
            log("❌ DIARIZATION FAILED")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            sendError("diarization_failed", message: "Diarization failed: \(error.localizedDescription)", encoder: encoder)
        }

        log("───────────────────────────────────────────────────────")
    }

    nonisolated static func sendResponse<T: Encodable>(_ response: T, encoder: JSONEncoder) {
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

    nonisolated static func sendError(_ code: String, message: String, encoder: JSONEncoder) {
        sendResponse(ErrorResponse(code: code, message: message), encoder: encoder)
    }

    nonisolated static func sendProgress(_ progress: DownloadUtils.DownloadProgress, encoder: JSONEncoder) {
        let phase: String
        switch progress.phase {
        case .listing:
            phase = "listing"
        case .downloading(let completedFiles, let totalFiles):
            phase = "downloading \(completedFiles)/\(totalFiles)"
        case .compiling(let modelName):
            phase = modelName.isEmpty ? "compiling" : "compiling \(modelName)"
        }

        sendResponse(
            ProgressResponse(
                progress: max(0.0, min(1.0, progress.fractionCompleted)),
                phase: phase
            ),
            encoder: encoder
        )
    }

    static func parseModelVersion(_ value: Any?) -> SupportedModelVersion? {
        guard let str = (value as? String)?.lowercased() else {
            return nil
        }

        switch str {
        case "v2":
            return .v2
        case "v3":
            return .v3
        default:
            return nil
        }
    }
}

