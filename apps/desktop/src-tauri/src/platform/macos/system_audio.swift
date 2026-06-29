import Foundation
import ScreenCaptureKit
import CoreMedia
import CoreAudio
import CoreGraphics

private struct HelperStatus: Codable {
    let available: Bool
    let permissionStatus: String
    let requiresRestart: Bool
    let errorCode: String?
}

private func writeStatus(_ status: HelperStatus) {
    let encoder = JSONEncoder()
    encoder.keyEncodingStrategy = .useDefaultKeys
    guard let data = try? encoder.encode(status) else { exit(5) }
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([0x0a]))
}

private func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
    var little = value.littleEndian
    withUnsafeBytes(of: &little) { data.append(contentsOf: $0) }
}

private func writeCaptureReady() {
    var packet = Data("AMAF".utf8)
    appendLE(UInt32(48_000), to: &packet)
    appendLE(UInt16(1), to: &packet)
    appendLE(Int64(-1), to: &packet)
    appendLE(UInt32(0), to: &packet)
    FileHandle.standardOutput.write(packet)
}

private final class AudioSink: NSObject, SCStreamOutput {
    private var firstPresentationSeconds: Double?

    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of type: SCStreamOutputType
    ) {
        guard type == .audio, sampleBuffer.isValid else { return }
        guard let format = CMSampleBufferGetFormatDescription(sampleBuffer),
              let description = CMAudioFormatDescriptionGetStreamBasicDescription(format)?.pointee else { return }

        var requiredSize = 0
        var retainedBlock: CMBlockBuffer?
        let sizingStatus = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: &requiredSize,
            bufferListOut: nil,
            bufferListSize: 0,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment),
            blockBufferOut: &retainedBlock
        )
        guard sizingStatus == noErr, requiredSize >= MemoryLayout<AudioBufferList>.size else { return }

        let raw = UnsafeMutableRawPointer.allocate(
            byteCount: requiredSize,
            alignment: MemoryLayout<AudioBufferList>.alignment
        )
        defer { raw.deallocate() }
        let listPointer = raw.bindMemory(to: AudioBufferList.self, capacity: 1)
        retainedBlock = nil
        let listStatus = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: listPointer,
            bufferListSize: requiredSize,
            blockBufferAllocator: kCFAllocatorDefault,
            blockBufferMemoryAllocator: kCFAllocatorDefault,
            flags: UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment),
            blockBufferOut: &retainedBlock
        )
        guard listStatus == noErr else { return }

        let buffers = UnsafeMutableAudioBufferListPointer(listPointer)
        guard !buffers.isEmpty else { return }
        let frameCount = CMSampleBufferGetNumSamples(sampleBuffer)
        let channelCount = max(1, Int(description.mChannelsPerFrame))
        let nonInterleaved = (description.mFormatFlags & kAudioFormatFlagIsNonInterleaved) != 0
        let isFloat = (description.mFormatFlags & kAudioFormatFlagIsFloat) != 0
        let isSignedInteger = (description.mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0
        let bits = Int(description.mBitsPerChannel)
        guard frameCount > 0, (isFloat || isSignedInteger), [16, 32].contains(bits) else { return }

        var mono = [Int16]()
        mono.reserveCapacity(frameCount)
        let bytesPerSample = bits / 8

        for frame in 0..<frameCount {
            var sum: Float = 0
            var used = 0
            for channel in 0..<channelCount {
                let bufferIndex = nonInterleaved ? min(channel, buffers.count - 1) : 0
                guard bufferIndex >= 0, bufferIndex < buffers.count,
                      let base = buffers[bufferIndex].mData else { continue }
                let sampleIndex = nonInterleaved ? frame : frame * channelCount + channel
                let offset = sampleIndex * bytesPerSample
                guard offset + bytesPerSample <= Int(buffers[bufferIndex].mDataByteSize) else { continue }
                let pointer = base.advanced(by: offset)
                let value: Float
                if isFloat && bits == 32 {
                    value = pointer.assumingMemoryBound(to: Float.self).pointee
                } else if isSignedInteger && bits == 16 {
                    value = Float(pointer.assumingMemoryBound(to: Int16.self).pointee) / Float(Int16.max)
                } else if isSignedInteger && bits == 32 {
                    value = Float(pointer.assumingMemoryBound(to: Int32.self).pointee) / Float(Int32.max)
                } else {
                    continue
                }
                sum += value
                used += 1
            }
            let averaged = used > 0 ? sum / Float(used) : 0
            let clamped = max(-1, min(1, averaged))
            mono.append(Int16(clamped * Float(Int16.max)))
        }

        guard !mono.isEmpty else { return }
        let presentation = CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sampleBuffer))
        if firstPresentationSeconds == nil, presentation.isFinite { firstPresentationSeconds = presentation }
        let relative = max(0, presentation - (firstPresentationSeconds ?? presentation))
        let timestampMs = Int64(relative * 1000)

        var packet = Data("AMAF".utf8)
        appendLE(UInt32(description.mSampleRate.rounded()), to: &packet)
        appendLE(UInt16(1), to: &packet)
        appendLE(timestampMs, to: &packet)
        appendLE(UInt32(mono.count), to: &packet)
        mono.withUnsafeBufferPointer { buffer in
            packet.append(contentsOf: UnsafeRawBufferPointer(buffer))
        }
        FileHandle.standardOutput.write(packet)
    }
}

private func probe() async {
    guard CGPreflightScreenCaptureAccess() else {
        writeStatus(HelperStatus(
            available: false,
            permissionStatus: "required",
            requiresRestart: false,
            errorCode: "ERR_SYSTEM_AUDIO_PERMISSION_REQUIRED"
        ))
        return
    }
    do {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
        guard !content.displays.isEmpty else {
            writeStatus(HelperStatus(
                available: false,
                permissionStatus: "authorized",
                requiresRestart: false,
                errorCode: "ERR_SYSTEM_AUDIO_UNAVAILABLE"
            ))
            return
        }
        writeStatus(HelperStatus(
            available: true,
            permissionStatus: "authorized",
            requiresRestart: false,
            errorCode: nil
        ))
    } catch {
        writeStatus(HelperStatus(
            available: false,
            permissionStatus: "authorized",
            requiresRestart: false,
            errorCode: "ERR_SYSTEM_AUDIO_UNAVAILABLE"
        ))
    }
}

private func requestPermission() async {
    let granted = CGRequestScreenCaptureAccess()
    writeStatus(HelperStatus(
        available: false,
        permissionStatus: granted ? "authorized" : "denied",
        requiresRestart: granted,
        errorCode: granted ? "ERR_SYSTEM_AUDIO_RESTART_REQUIRED" : "ERR_SYSTEM_AUDIO_PERMISSION_DENIED"
    ))
}

private func capture() async throws {
    guard CGPreflightScreenCaptureAccess() else { throw NSError(domain: "AccordMesh", code: 2) }
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
    guard let display = content.displays.first else { throw NSError(domain: "AccordMesh", code: 3) }
    let filter = SCContentFilter(display: display, excludingWindows: [])
    let configuration = SCStreamConfiguration()
    configuration.capturesAudio = true
    configuration.excludesCurrentProcessAudio = true
    configuration.sampleRate = 48_000
    configuration.channelCount = 2
    configuration.width = 2
    configuration.height = 2
    configuration.showsCursor = false
    configuration.queueDepth = 3
    configuration.minimumFrameInterval = CMTime(value: 1, timescale: 2)

    let sink = AudioSink()
    let stream = SCStream(filter: filter, configuration: configuration, delegate: nil)
    try stream.addStreamOutput(
        sink,
        type: .audio,
        sampleHandlerQueue: DispatchQueue(label: "org.accordmesh.system-audio", qos: .userInitiated)
    )
    try await stream.startCapture()
    writeCaptureReady()
    while true { try await Task.sleep(nanoseconds: 1_000_000_000) }
}

@main
struct AccordMeshSystemAudio {
    static func main() async {
        let argument = CommandLine.arguments.dropFirst().first ?? "--probe"
        switch argument {
        case "--probe":
            await probe()
        case "--request-permission":
            await requestPermission()
        case "--capture":
            do { try await capture() } catch { exit(4) }
        default:
            exit(6)
        }
    }
}
