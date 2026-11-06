import AppKit

// Disk image window, in points. Keep these in sync with
// bundle.macOS.dmg in tauri.conf.json. Finder places each icon by its center
// and draws the icon names in black, so each name sits on a light card.
let windowWidth = 660
let windowHeight = 440
let iconSize: CGFloat = 128
let appCenter = CGPoint(x: 180, y: 190)
let applicationsCenter = CGPoint(x: 480, y: 190)

let scale = 2
guard let rep = NSBitmapImageRep(
  bitmapDataPlanes: nil,
  pixelsWide: windowWidth * scale,
  pixelsHigh: windowHeight * scale,
  bitsPerSample: 8,
  samplesPerPixel: 4,
  hasAlpha: true,
  isPlanar: false,
  colorSpaceName: .deviceRGB,
  bytesPerRow: 0,
  bitsPerPixel: 0
) else {
  fputs("Could not create bitmap\n", stderr)
  exit(1)
}
rep.size = NSSize(width: windowWidth, height: windowHeight)

NSGraphicsContext.saveGraphicsState()
guard let graphics = NSGraphicsContext(bitmapImageRep: rep) else {
  fputs("Could not create graphics context\n", stderr)
  exit(1)
}
NSGraphicsContext.current = graphics
graphics.imageInterpolation = .high

func yFromTop(_ y: CGFloat) -> CGFloat {
  CGFloat(windowHeight) - y
}

let field = NSColor(srgbRed: 0.075, green: 0.078, blue: 0.102, alpha: 1)
let card = NSColor(srgbRed: 0.965, green: 0.961, blue: 0.953, alpha: 1)
let titleColor = NSColor(srgbRed: 0.96, green: 0.96, blue: 0.97, alpha: 1)
let noteColor = NSColor(srgbRed: 0.73, green: 0.74, blue: 0.78, alpha: 1)
let arrowColor = NSColor(srgbRed: 0.96, green: 0.71, blue: 0.22, alpha: 1)

field.setFill()
NSRect(x: 0, y: 0, width: windowWidth, height: windowHeight).fill()

func cardRect(centerX: CGFloat) -> NSRect {
  let width: CGFloat = 204
  let top = appCenter.y - iconSize / 2 - 16
  let bottom = appCenter.y + iconSize / 2 + 44
  return NSRect(x: centerX - width / 2, y: yFromTop(bottom), width: width, height: bottom - top)
}

for center in [appCenter, applicationsCenter] {
  let rect = cardRect(centerX: center.x)
  NSColor(srgbRed: 0, green: 0, blue: 0, alpha: 0.28).setFill()
  NSBezierPath(roundedRect: rect.offsetBy(dx: 0, dy: -8), xRadius: 28, yRadius: 28).fill()
  card.setFill()
  NSBezierPath(roundedRect: rect, xRadius: 28, yRadius: 28).fill()
}

func drawCentered(_ text: String, bottomFromTop: CGFloat, font: NSFont, color: NSColor) -> CGFloat {
  let line = NSAttributedString(
    string: text,
    attributes: [.font: font, .foregroundColor: color]
  )
  let size = line.size()
  line.draw(
    in: NSRect(
      x: (CGFloat(windowWidth) - size.width) / 2,
      y: yFromTop(bottomFromTop),
      width: size.width,
      height: size.height
    )
  )
  print("\(text) \(Int(size.width.rounded()))")
  return size.width
}

_ = drawCentered(
  "Drag to Applications",
  bottomFromTop: 78,
  font: NSFont.systemFont(ofSize: 22, weight: .semibold),
  color: titleColor
)

arrowColor.setFill()
let midY = yFromTop(appCenter.y)
NSBezierPath(roundedRect: NSRect(x: 304, y: midY - 5, width: 40, height: 10), xRadius: 5, yRadius: 5).fill()
let head = NSBezierPath()
head.move(to: NSPoint(x: 336, y: midY - 18))
head.line(to: NSPoint(x: 370, y: midY))
head.line(to: NSPoint(x: 336, y: midY + 18))
head.close()
head.fill()

let noteFont = NSFont.systemFont(ofSize: 13, weight: .regular)
_ = drawCentered(
  "If macOS blocks the first open,",
  bottomFromTop: 348,
  font: noteFont,
  color: noteColor
)
_ = drawCentered(
  "choose Open Anyway in Privacy & Security.",
  bottomFromTop: 368,
  font: noteFont,
  color: noteColor
)

NSGraphicsContext.restoreGraphicsState()

guard let png = rep.representation(using: .png, properties: [:]) else {
  fputs("Could not encode background\n", stderr)
  exit(1)
}
try png.write(to: URL(fileURLWithPath: "background.png"))
