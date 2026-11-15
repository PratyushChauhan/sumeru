import Foundation

/// Anything that answers Sumeru API calls: the real `sumeru engine` process, or a mock.
public protocol Engine: Sendable {
    /// Inputs: method name (Rust `api::dispatch`), JSON-encodable params.
    /// Outputs: the raw JSON `result`, or throws `EngineError` with the engine's message.
    func raw(_ method: String, _ params: (any Encodable & Sendable)?) async throws -> Data
}

public struct EngineError: LocalizedError, Equatable {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

extension Engine {
    /// Inputs: method, optional params. Outputs: the result decoded as `T`.
    public func call<T: Decodable>(_ method: String, _ params: (any Encodable & Sendable)? = nil) async throws -> T {
        try JSONDecoder().decode(T.self, from: try await raw(method, params))
    }

    /// Inputs: method, optional params. Outputs: none (for commands that return null).
    public func run(_ method: String, _ params: (any Encodable & Sendable)? = nil) async throws {
        _ = try await raw(method, params)
    }
}

/// Drives `sumeru engine` over pipes: one JSON request per line on stdin, replies matched by id
/// on stdout. The engine (and the gateway inside it) exits when this process closes its stdin.
public actor EngineProcess: Engine {
    private let executable: URL
    private var process: Process?
    private var input: FileHandle?
    private var buffer = Data()
    private var nextID = 1
    private var pending: [Int: CheckedContinuation<Data, Error>] = [:]

    /// Inputs: path to the `sumeru` binary (bundled at `Contents/Helpers/sumeru`).
    public init(executable: URL) { self.executable = executable }

    public func raw(_ method: String, _ params: (any Encodable & Sendable)?) async throws -> Data {
        try start()
        let id = nextID
        nextID += 1
        var line = try JSONSerialization.data(withJSONObject: [
            "id": id, "method": method,
            "params": try params.map { try JSONSerialization.jsonObject(with: JSONEncoder().encode($0), options: .fragmentsAllowed) } ?? NSNull(),
        ])
        line.append(0x0A)
        return try await withCheckedThrowingContinuation { continuation in
            pending[id] = continuation
            input?.write(line)
        }
    }

    /// Inputs: none. Outputs: a running engine child (relaunched if it exited).
    private func start() throws {
        if process?.isRunning == true { return }
        let p = Process()
        p.executableURL = executable
        p.arguments = ["engine"]
        let stdin = Pipe(), stdout = Pipe()
        p.standardInput = stdin
        p.standardOutput = stdout
        stdout.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            Task { await self?.receive(data) }
        }
        p.terminationHandler = { [weak self] _ in Task { await self?.failAll("Sumeru engine stopped") } }
        try p.run()
        process = p
        input = stdin.fileHandleForWriting
    }

    /// Inputs: stdout bytes. Outputs: completed calls for every full reply line.
    private func receive(_ data: Data) {
        buffer.append(data)
        while let nl = buffer.firstIndex(of: 0x0A) {
            let line = buffer[buffer.startIndex..<nl]
            buffer.removeSubrange(buffer.startIndex...nl)
            guard let reply = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                  let id = reply["id"] as? Int, let continuation = pending.removeValue(forKey: id)
            else { continue }
            if let error = reply["error"] as? String {
                continuation.resume(throwing: EngineError(error))
            } else {
                let result = reply["result"] ?? NSNull()
                continuation.resume(with: Result {
                    try JSONSerialization.data(withJSONObject: result, options: .fragmentsAllowed)
                })
            }
        }
    }

    /// Inputs: reason. Outputs: every waiting call failed (the engine exited).
    private func failAll(_ reason: String) {
        for (_, continuation) in pending { continuation.resume(throwing: EngineError(reason)) }
        pending.removeAll()
        process = nil
    }

    /// Inputs: none. Outputs: engine stdin closed, so it stops the gateway and exits.
    public func stop() {
        try? input?.close()
        input = nil
    }
}

