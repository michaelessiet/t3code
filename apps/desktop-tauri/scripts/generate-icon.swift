// Generates the Vitre app icon: a glazed-glass "V" over a translucent
// frosted backdrop. Output is 1024x1024 8-bit RGBA PNG (Tauri's embedded
// window icon requires 8-bit; 16-bit sources fail at window creation with
// "invalid icon ... pixel count").
//
// Usage: swift generate-icon.swift <output.png>
// Regenerate the .icns afterwards (see apps/desktop-tauri/README.md).

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let size = 1024
let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
guard
    let context = CGContext(
        data: nil,
        width: size,
        height: size,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )
else {
    fatalError("could not create CGContext")
}

func rgba(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat) -> CGColor {
    CGColor(colorSpace: colorSpace, components: [r, g, b, a])!
}

func gradient(_ stops: [(CGColor, CGFloat)]) -> CGGradient {
    CGGradient(
        colorsSpace: colorSpace,
        colors: stops.map(\.0) as CFArray,
        locations: stops.map(\.1)
    )!
}

let s = CGFloat(size)

// ---- Translucent frosted backdrop (macOS-style rounded square) ----
let inset: CGFloat = 100
let backdrop = CGPath(
    roundedRect: CGRect(x: inset, y: inset, width: s - 2 * inset, height: s - 2 * inset),
    cornerWidth: 185,
    cornerHeight: 185,
    transform: nil
)

context.saveGState()
context.addPath(backdrop)
context.clip()
context.drawLinearGradient(
    gradient([
        (rgba(0.93, 0.96, 1.00, 0.30), 0.0),
        (rgba(0.80, 0.88, 0.98, 0.16), 0.55),
        (rgba(0.72, 0.82, 0.95, 0.22), 1.0),
    ]),
    start: CGPoint(x: s / 2, y: s - inset),
    end: CGPoint(x: s / 2, y: inset),
    options: []
)
context.restoreGState()

// Glass edge of the backdrop panel.
context.saveGState()
context.addPath(backdrop)
context.setStrokeColor(rgba(1, 1, 1, 0.42))
context.setLineWidth(4)
context.strokePath()
context.restoreGState()

// ---- The V: a thick round-capped stroke turned into a fill region ----
let vPath = CGMutablePath()
vPath.move(to: CGPoint(x: 338, y: 712))
vPath.addLine(to: CGPoint(x: 512, y: 320))
vPath.addLine(to: CGPoint(x: 686, y: 712))
// normalized() unions the stroke outline's overlapping contours; without it
// the vertex overlap leaves an inner seam triangle in the edge highlight.
let vBody = vPath.copy(
    strokingWithWidth: 128,
    lineCap: .round,
    lineJoin: .round,
    miterLimit: 10
).normalized(using: .winding)

// Soft depth shadow under the glass slab.
context.saveGState()
context.setShadow(
    offset: CGSize(width: 0, height: -14),
    blur: 34,
    color: rgba(0.10, 0.22, 0.42, 0.35)
)
context.addPath(vBody)
context.setFillColor(rgba(1, 1, 1, 0.01))
context.fillPath()
context.restoreGState()

// Glass slab fill: bright frosted white fading into an azure tint.
context.saveGState()
context.addPath(vBody)
context.clip()
context.drawLinearGradient(
    gradient([
        (rgba(1.00, 1.00, 1.00, 0.97), 0.0),
        (rgba(0.78, 0.89, 1.00, 0.88), 0.42),
        (rgba(0.38, 0.62, 0.94, 0.85), 1.0),
    ]),
    start: CGPoint(x: s / 2, y: 780),
    end: CGPoint(x: s / 2, y: 260),
    options: []
)

// Sheen: a brighter band across the upper half of the glyph.
context.drawLinearGradient(
    gradient([
        (rgba(1, 1, 1, 0.55), 0.0),
        (rgba(1, 1, 1, 0.10), 0.30),
        (rgba(1, 1, 1, 0.00), 0.50),
    ]),
    start: CGPoint(x: s / 2, y: 780),
    end: CGPoint(x: s / 2, y: 260),
    options: []
)
context.restoreGState()

// Glass edge highlight around the glyph.
context.saveGState()
context.addPath(vBody)
context.setStrokeColor(rgba(1, 1, 1, 0.75))
context.setLineWidth(5)
context.strokePath()
context.restoreGState()

// ---- Write PNG ----
let arguments = CommandLine.arguments
guard arguments.count == 2 else {
    fatalError("usage: swift generate-icon.swift <output.png>")
}
let output = URL(fileURLWithPath: arguments[1])
guard let image = context.makeImage(),
    let destination = CGImageDestinationCreateWithURL(
        output as CFURL, UTType.png.identifier as CFString, 1, nil)
else {
    fatalError("could not create image destination")
}
CGImageDestinationAddImage(destination, image, nil)
guard CGImageDestinationFinalize(destination) else {
    fatalError("could not write \(output.path)")
}
print("wrote \(output.path)")
