import Foundation
import Darwin
import FluidAudio

// Keep a duplicate of the real protocol stdout so progress events still reach
// Tauri while native library calls temporarily redirect STDOUT_FILENO.

let protocolStdoutFileDescriptor = dup(STDOUT_FILENO)

func writeProtocolLine(_ line: String) {
    let outputFileDescriptor = protocolStdoutFileDescriptor >= 0 ? protocolStdoutFileDescriptor : STDOUT_FILENO
    var data = Data(line.utf8)
    data.append(0x0A)

    data.withUnsafeBytes { buffer in
        guard let baseAddress = buffer.baseAddress else {
            return
        }

        var bytesWritten = 0
        while bytesWritten < buffer.count {
            let result = Darwin.write(
                outputFileDescriptor,
                baseAddress.advanced(by: bytesWritten),
                buffer.count - bytesWritten
            )
            if result <= 0 {
                return
            }
            bytesWritten += result
        }
    }
}

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

// FluidAudio/CoreML can write diagnostics directly to stdout from native code.
// Stdout is our line-delimited JSON protocol, so run library calls with stdout
// temporarily redirected to stderr and restore it before sending responses.
@MainActor
func withLibraryStdoutRedirected<T>(_ operation: () async throws -> T) async throws -> T {
    fflush(stdout)
    let savedStdout = dup(STDOUT_FILENO)
    guard savedStdout >= 0 else {
        return try await operation()
    }

    if dup2(STDERR_FILENO, STDOUT_FILENO) < 0 {
        close(savedStdout)
        return try await operation()
    }

    defer {
        fflush(stdout)
        dup2(savedStdout, STDOUT_FILENO)
        close(savedStdout)
    }

    return try await operation()
}

// Log system information for debugging
func logSystemInfo() {
    log("🦜 Parakeet sidecar started")
    log("   Architecture: \(getArchitectureInfo())")
    log("   macOS: \(ProcessInfo.processInfo.operatingSystemVersionString)")
    log("   PID: \(ProcessInfo.processInfo.processIdentifier)")
}


struct IncomingVocabularyTerm: Decodable {
    let text: String
    let aliases: [String]

    private enum CodingKeys: String, CodingKey {
        case text, aliases
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        text = try container.decode(String.self, forKey: .text)
        aliases = try container.decodeIfPresent([String].self, forKey: .aliases) ?? []
    }
}

struct OkResponse: Encodable {
    let type: String = "ok"
    let command: String
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
    let customVocabularySupported: Bool = true
    let customVocabularyReady: Bool = ctcVocabularyReady()
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
@MainActor var cachedCtcModels: CtcModels?
@MainActor var cachedCtcTokenizer: CtcTokenizer?

func ctcVocabularyReady() -> Bool {
    let directory = CtcModels.defaultCacheDirectory(for: .ctc110m)
    let tokenizerURL = directory.appendingPathComponent("tokenizer.json")
    return CtcModels.modelsExist(at: directory)
        && FileManager.default.fileExists(atPath: tokenizerURL.path)
}
@MainActor var cachedCtcSpotter: CtcKeywordSpotter?
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
            await transcribeFile(audioPath, language: nil, translateToEnglish: false, customVocabulary: [], encoder: encoder)
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
                        let customVocabulary = decodeCustomVocabulary(from: data)
                        await transcribeFile(audioPath, language: language, translateToEnglish: translateToEnglish, customVocabulary: customVocabulary, encoder: encoder)
                    } else {
                        sendError("missing_audio_path", message: "audio_path is required", encoder: encoder)
                    }


                case "download_ctc_models":
                    await downloadCtcModels(encoder: encoder)

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
                models = try await withLibraryStdoutRedirected {
                    try await AsrModels.downloadAndLoad(version: version.asrVersion, progressHandler: progressHandler)
                }
                downloadedVersions.insert(version)
                log("✅ Download complete for \(version.rawValue.uppercased())")
            } else {
                log("🔍 Attempting to load Parakeet \(version.rawValue.uppercased()) from cache...")
                do {
                    models = try await withLibraryStdoutRedirected {
                        try await AsrModels.loadFromCache(version: version.asrVersion, progressHandler: progressHandler)
                    }
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
            try await withLibraryStdoutRedirected {
                try await manager.loadModels(models)
            }
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

    static func transcribeFile(_ audioPath: String, language: String? = nil, translateToEnglish: Bool = false, customVocabulary: [IncomingVocabularyTerm] = [], encoder: JSONEncoder) async {
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
            let result = try await withLibraryStdoutRedirected {
                try await manager.transcribe(fileURL, decoderState: &decoderState)
            }

            let elapsed = Date().timeIntervalSince(startTime)
            log("✅ Transcription complete in \(String(format: "%.2f", elapsed))s")
            log("📝 Result text length: \(result.text.count) chars")
            log("⏱️ Audio duration: \(result.duration)s")

            let finalText = await rescoreTranscriptIfPossible(
                result: result,
                audioURL: fileURL,
                customVocabulary: customVocabulary
            )

            // Send transcription response
            let response = TranscriptionResponse(
                text: finalText,
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


    static func downloadCtcModels(encoder: JSONEncoder) async {
        log("───────────────────────────────────────────────────────")
        log("📥 DOWNLOAD CTC MODELS REQUEST")
        log("───────────────────────────────────────────────────────")

        do {
            sendResponse(ProgressResponse(progress: 0.0, phase: "downloading ctc models"), encoder: encoder)
            try await CtcModels.download(variant: .ctc110m)

            guard ctcVocabularyReady() else {
                sendError("ctc_model_download_failed", message: "CTC model download completed but required files are missing", encoder: encoder)
                return
            }

            cachedCtcModels = nil
            cachedCtcTokenizer = nil
            cachedCtcSpotter = nil
            sendResponse(ProgressResponse(progress: 1.0, phase: "ctc models ready"), encoder: encoder)
            sendResponse(OkResponse(command: "download_ctc_models"), encoder: encoder)
        } catch {
            log("❌ CTC MODEL DOWNLOAD FAILED")
            log("❌ Error type: \(type(of: error))")
            log("❌ Error details: \(error)")
            log("❌ Localized: \(error.localizedDescription)")
            sendError("ctc_model_download_failed", message: "Failed to download CTC models: \(error.localizedDescription)", encoder: encoder)
        }

        log("───────────────────────────────────────────────────────")
    }

    static func rescoreTranscriptIfPossible(
        result: ASRResult,
        audioURL: URL,
        customVocabulary: [IncomingVocabularyTerm]
    ) async -> String {
        guard !customVocabulary.isEmpty else {
            return result.text
        }

        guard ctcVocabularyReady() else {
            log("ℹ️ Custom vocabulary skipped: CTC models not ready")
            return result.text
        }

        let directory = CtcModels.defaultCacheDirectory(for: .ctc110m)

        guard let tokenTimings = result.tokenTimings, !tokenTimings.isEmpty else {
            log("ℹ️ Custom vocabulary skipped: token timings unavailable")
            return result.text
        }

        do {
            let tokenizer = try await cachedOrLoadCtcTokenizer(from: directory)
            let terms = customVocabulary.compactMap { term -> CustomVocabularyTerm? in
                let tokenIds = tokenizer.encode(term.text)
                guard !tokenIds.isEmpty else { return nil }
                return CustomVocabularyTerm(
                    text: term.text,
                    aliases: term.aliases.isEmpty ? nil : term.aliases,
                    tokenIds: nil,
                    ctcTokenIds: tokenIds
                )
            }

            guard !terms.isEmpty else {
                log("ℹ️ Custom vocabulary skipped: no tokenizable terms")
                return result.text
            }

            let vocabulary = CustomVocabularyContext(terms: terms, minTermLength: 3)
            let models = try await cachedOrLoadCtcModels(from: directory)
            let spotter = cachedOrCreateCtcSpotter(models: models)
            let samples = try AudioConverter().resampleAudioFile(audioURL)
            let spot = try await spotter.spotKeywordsWithLogProbs(
                audioSamples: samples,
                customVocabulary: vocabulary
            )
            // Term values must never be logged. FluidAudio exposes no runtime logger level;
            // shipped sidecars are built in release so VocabularyRescorer DEBUG logs stay compiled out.
            let rescorer = try await VocabularyRescorer.create(
                spotter: spotter,
                vocabulary: vocabulary,
                ctcModelDirectory: directory
            )
            let output = rescorer.ctcTokenRescore(
                transcript: result.text,
                tokenTimings: tokenTimings,
                logProbs: spot.logProbs,
                frameDuration: spot.frameDuration
            )

            if output.wasModified {
                log("✅ Custom vocabulary applied")
                return output.text
            }

            log("ℹ️ Custom vocabulary produced no transcript changes")
            return result.text
        } catch {
            log("⚠️ Custom vocabulary rescore failed; returning original transcript. Error type: \(type(of: error))")
            return result.text
        }
    }

    static func cachedOrLoadCtcModels(from directory: URL) async throws -> CtcModels {
        if let models = cachedCtcModels {
            return models
        }

        let models = try await CtcModels.load(from: directory, variant: .ctc110m)
        cachedCtcModels = models
        cachedCtcSpotter = CtcKeywordSpotter(models: models, blankId: models.vocabulary.count)
        return models
    }

    static func cachedOrLoadCtcTokenizer(from directory: URL) async throws -> CtcTokenizer {
        if let tokenizer = cachedCtcTokenizer {
            return tokenizer
        }

        let tokenizer = try await CtcTokenizer.load(from: directory)
        cachedCtcTokenizer = tokenizer
        return tokenizer
    }

    static func cachedOrCreateCtcSpotter(models: CtcModels) -> CtcKeywordSpotter {
        if let spotter = cachedCtcSpotter {
            return spotter
        }

        let spotter = CtcKeywordSpotter(models: models, blankId: models.vocabulary.count)
        cachedCtcSpotter = spotter
        return spotter
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
                writeProtocolLine(jsonString)
            }
        } catch {
            writeProtocolLine("{\"type\":\"error\",\"code\":\"serialization_error\",\"message\":\"Failed to serialize response\"}")
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


    static func decodeCustomVocabulary(from data: Data) -> [IncomingVocabularyTerm] {
        struct TranscribeCommand: Decodable {
            let custom_vocabulary: [IncomingVocabularyTerm]?
        }

        do {
            return try JSONDecoder().decode(TranscribeCommand.self, from: data).custom_vocabulary ?? []
        } catch {
            log("⚠️ Custom vocabulary ignored: command vocabulary payload could not be decoded")
            return []
        }
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

