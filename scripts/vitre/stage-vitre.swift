import AppKit

// Stage the Vitre window for a screenshot: gpui does not draw an occluded
// window, so every other regular app is hidden first, Vitre is activated, and
// the window id is printed for `screencapture -l`.
let mode = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "stage"
let apps = NSWorkspace.shared.runningApplications.filter {
    $0.activationPolicy == .regular
}

func vitre() -> NSRunningApplication? {
    apps.first { ($0.localizedName ?? "").contains("vitre") || ($0.bundleIdentifier ?? "").contains("vitre") }
}

switch mode {
case "unhide":
    for app in apps { app.unhide() }
case "stage":
    guard let target = vitre() else {
        FileHandle.standardError.write("vitre is not running\n".data(using: .utf8)!)
        exit(1)
    }
    for app in apps where app.processIdentifier != target.processIdentifier {
        app.hide()
    }
    target.activate(options: [.activateAllWindows])
    usleep(1_200_000)
    let info = CGWindowListCopyWindowInfo([.optionAll, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? []
    for window in info {
        let owner = window[kCGWindowOwnerName as String] as? String ?? ""
        let pid = window[kCGWindowOwnerPID as String] as? Int32 ?? -1
        guard pid == target.processIdentifier else { continue }
        let number = window[kCGWindowNumber as String] as? Int ?? -1
        let bounds = window[kCGWindowBounds as String] as? [String: Any] ?? [:]
        let w = bounds["Width"] as? Double ?? 0
        let h = bounds["Height"] as? Double ?? 0
        // Skip the tiny helper surfaces (shadows, tooltips).
        if w > 400 && h > 300 {
            print("\(number) \(owner) \(Int(w))x\(Int(h))")
        }
    }
default:
    FileHandle.standardError.write("usage: stage-vitre.swift [stage|unhide]\n".data(using: .utf8)!)
    exit(2)
}
