#!/usr/bin/env swift
// Original network-guard icon. AppKit keeps generation independent of third-party packages.
import AppKit

let side = 1024
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
let context = NSGraphicsContext(bitmapImageRep: bitmap)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
context.cgContext.setAllowsAntialiasing(true)
context.cgContext.setShouldAntialias(true)
NSColor.clear.setFill()
NSRect(x: 0, y: 0, width: side, height: side).fill()
let tile = NSBezierPath(roundedRect: NSRect(x: 100, y: 100, width: 824, height: 824),
    xRadius: 184, yRadius: 184)
NSGraphicsContext.saveGraphicsState()
let shadow = NSShadow()
shadow.shadowColor = NSColor.black.withAlphaComponent(0.23)
shadow.shadowBlurRadius = 18
shadow.shadowOffset = NSSize(width: 0, height: -10)
shadow.set()
NSColor(calibratedRed: 0.06, green: 0.14, blue: 0.27, alpha: 1).setFill()
tile.fill()
NSGraphicsContext.restoreGraphicsState()
NSGradient(starting: NSColor(calibratedRed: 0.11, green: 0.27, blue: 0.45, alpha: 1),
    ending: NSColor(calibratedRed: 0.04, green: 0.10, blue: 0.22, alpha: 1))!
    .draw(in: tile, angle: -90)
let inset = NSBezierPath(roundedRect: NSRect(x: 101, y: 101, width: 822, height: 822),
    xRadius: 183, yRadius: 183)
NSColor.white.withAlphaComponent(0.10).setStroke()
inset.lineWidth = 2
inset.stroke()
let shield = NSBezierPath()
shield.move(to: NSPoint(x: 512, y: 768))
shield.curve(to: NSPoint(x: 700, y: 691), controlPoint1: NSPoint(x: 563, y: 738), controlPoint2: NSPoint(x: 650, y: 711))
shield.line(to: NSPoint(x: 688, y: 499))
shield.curve(to: NSPoint(x: 512, y: 256), controlPoint1: NSPoint(x: 681, y: 382), controlPoint2: NSPoint(x: 586, y: 298))
shield.curve(to: NSPoint(x: 336, y: 499), controlPoint1: NSPoint(x: 438, y: 298), controlPoint2: NSPoint(x: 343, y: 382))
shield.line(to: NSPoint(x: 324, y: 691))
shield.curve(to: NSPoint(x: 512, y: 768), controlPoint1: NSPoint(x: 374, y: 711), controlPoint2: NSPoint(x: 461, y: 738))
shield.close()
NSColor(calibratedRed: 0.17, green: 0.84, blue: 0.79, alpha: 1).setStroke()
shield.lineWidth = 42
shield.lineJoinStyle = .round
shield.stroke()
let check = NSBezierPath()
check.move(to: NSPoint(x: 409, y: 514))
check.line(to: NSPoint(x: 482, y: 439))
check.line(to: NSPoint(x: 620, y: 594))
check.lineWidth = 53
check.lineCapStyle = .round
check.lineJoinStyle = .round
NSColor(calibratedRed: 0.96, green: 0.99, blue: 1, alpha: 1).setStroke()
check.stroke()
NSGraphicsContext.restoreGraphicsState()
let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/icon.png"
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output))
print("Generated original 1024px app icon")
