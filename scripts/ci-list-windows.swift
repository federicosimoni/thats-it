// Lists the on-screen windows owned by a process (used by CI to smoke-test the widget).
// Usage: swift scripts/ci-list-windows.swift <process-name>
// Prints one line per window: layer, bounds; exits 1 if none is found.
import CoreGraphics
import Foundation

let owner = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "thatsit"
let desktopIconLevel = Int(CGWindowLevelForKey(.desktopIconWindow))
print("desktop icon level = \(desktopIconLevel), expected widget level = \(desktopIconLevel + 1)")

let info = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] ?? []
var found = false
for w in info where (w[kCGWindowOwnerName as String] as? String) == owner {
    let layer = w[kCGWindowLayer as String] as? Int ?? -1
    let bounds = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    let onScreen = w[kCGWindowIsOnscreen as String] as? Bool ?? false
    print("window: layer=\(layer) onscreen=\(onScreen) bounds=\(bounds)")
    if layer == desktopIconLevel + 1 && onScreen { found = true }
}
if !found {
    print("no on-screen window at desktop icon level + 1 found for \(owner)")
    exit(1)
}
print("OK")
