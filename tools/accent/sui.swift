import AppKit
import SwiftUI

struct Board: View {
    @State var on = true
    @State var sw = true
    @State var v = 0.6
    @State var seg = 1
    @State var sel: Int? = 1
    @State var radio = 0
    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 10) {
                Toggle("Checkbox", isOn: $on).toggleStyle(.checkbox)
                Toggle("Switch", isOn: $sw).toggleStyle(.switch)
                Button("Prominent") {}.buttonStyle(.borderedProminent)
                Button("Default") {}.keyboardShortcut(.defaultAction)
                Picker("", selection: $seg) { Text("One").tag(0); Text("Two").tag(1); Text("Three").tag(2) }.pickerStyle(.segmented).frame(width: 200)
                Picker("", selection: $radio) { Text("A").tag(0); Text("B").tag(1) }.pickerStyle(.radioGroup).horizontalRadioGroupLayout()
                Slider(value: $v).frame(width: 200)
                ProgressView(value: 0.6).frame(width: 200)
                HStack { Rectangle().fill(Color.accentColor).frame(width: 60, height: 30); Rectangle().fill(.tint).frame(width: 60, height: 30); Image(systemName: "star.fill").foregroundStyle(.tint).font(.title); Link("Link", destination: URL(string: "https://example.com")!) }
            }
            List(selection: $sel) { ForEach(0..<4) { i in Label("Row \(i)", systemImage: "folder").tag(i) } }.frame(width: 180, height: 160)
            List(selection: $sel) { Section("Sidebar") { ForEach(0..<4) { i in Label("Item \(i)", systemImage: "folder").tag(i) } } }.listStyle(.sidebar).frame(width: 180, height: 160)
        }.padding(16)
    }
}

let yesBlock: @convention(block) (AnyObject) -> Bool = { _ in true }
let yesImp = imp_implementationWithBlock(yesBlock)
_ = NSApplication.shared
NSApp.setActivationPolicy(.regular)
for s in ["isKeyWindow", "isMainWindow", "_hasActiveControls", "_hasKeyAppearance", "_hasMainAppearance"] {
    if let m = class_getInstanceMethod(NSWindow.self, NSSelectorFromString(s)) { method_setImplementation(m, yesImp) }
}
method_setImplementation(class_getInstanceMethod(NSApplication.self, NSSelectorFromString("isActive"))!, yesImp)
if getenv("DARK") != nil { NSApp.appearance = NSAppearance(named: .darkAqua) }
if getenv("LIGHT") != nil { NSApp.appearance = NSAppearance(named: .aqua) }
typealias HookFn = @convention(c) () -> Void
let hook = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "hooks_install").map { unsafeBitCast($0, to: HookFn.self) }
if getenv("LATE") == nil { hook?() }
let w = NSWindow(contentRect: NSRect(x: 200, y: 200, width: 640, height: 330), styleMask: [.titled], backing: .buffered, defer: false)
w.contentView = NSHostingView(rootView: Board())
w.makeKeyAndOrderFront(nil)
NSApp.activate(ignoringOtherApps: true)
RunLoop.current.run(until: Date(timeIntervalSinceNow: 1.5))
if getenv("LATE") != nil { hook?(); RunLoop.current.run(until: Date(timeIntervalSinceNow: 1.0)) }
typealias CapFn = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
let cap = unsafeBitCast(dlsym(UnsafeMutableRawPointer(bitPattern: -2), "CGWindowListCreateImage")!, to: CapFn.self)
let img = cap(.null, 8, UInt32(w.windowNumber), 1 | 8)!.takeRetainedValue()
let rep = NSBitmapImageRep(cgImage: img)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
print("wrote", CommandLine.arguments[1])
