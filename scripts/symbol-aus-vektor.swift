// Zeichnet die App-Symbole direkt aus der Vektorgrafik des Atlas-Zeichens
// (PDF bzw. .ai, PDF-kompatibel) — jede Größe eigens, wie macOS es bei
// modernen Symbolen tut. Dadurch bleiben feine Linien (Nadel, Ring) scharf.
//
// Aufruf: swift scripts/symbol-aus-vektor.swift <logo.pdf> <ausgabe-ordner> <schwarz|weiss|gruen> [anteil]
//   anteil = Höhe des Zeichens relativ zur Symbolfläche (Standard 0.92)
// Erzeugt icon_16x16.png … icon_512x512@2x.png (iconset) und quelle-1024.png.
import AppKit
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 4, let pdf = CGPDFDocument(URL(fileURLWithPath: args[1]) as CFURL), let seite = pdf.page(at: 1) else {
  FileHandle.standardError.write("Aufruf: symbol-aus-vektor.swift <logo.pdf> <ordner> <schwarz|weiss|gruen> [anteil]\n".data(using: .utf8)!)
  exit(1)
}
let ordner = URL(fileURLWithPath: args[2])
let variante = args[3]
let anteil = args.count > 4 ? CGFloat(Double(args[4]) ?? 0.92) : 0.92
try? FileManager.default.createDirectory(at: ordner, withIntermediateDirectories: true)

let box = seite.getBoxRect(.cropBox)

/// Grenzen der tatsächlich gezeichneten Fläche (ohne Leerraum der Seite).
func inhaltsRahmen() -> CGRect {
  let s: CGFloat = 4
  let w = Int(box.width * s), h = Int(box.height * s)
  let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                      space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
  ctx.setFillColor(gray: 1, alpha: 1); ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
  ctx.scaleBy(x: s, y: s); ctx.translateBy(x: -box.minX, y: -box.minY); ctx.drawPDFPage(seite)
  let daten = ctx.data!.bindMemory(to: UInt8.self, capacity: w * h)
  var minX = w, minY = h, maxX = 0, maxY = 0
  let bpr = ctx.bytesPerRow
  for y in 0..<h { for x in 0..<w where daten[y * bpr + x] < 200 {
    minX = min(minX, x); maxX = max(maxX, x); minY = min(minY, y); maxY = max(maxY, y) } }
  // Bitmap-Zeilen laufen von oben; in PDF-Koordinaten umrechnen.
  return CGRect(x: box.minX + CGFloat(minX) / s, y: box.minY + CGFloat(h - 1 - maxY) / s,
                width: CGFloat(maxX - minX + 1) / s, height: CGFloat(maxY - minY + 1) / s)
}
let zeichen = inhaltsRahmen()

func farben() -> (flaeche: [CGColor], zeichen: CGColor) {
  switch variante {
  case "weiss": return ([CGColor(red: 0.98, green: 0.98, blue: 0.972, alpha: 1)], CGColor(red: 0.055, green: 0.055, blue: 0.055, alpha: 1))
  case "gruen": return ([CGColor(red: 0.102, green: 0.29, blue: 0.196, alpha: 1), CGColor(red: 0.035, green: 0.133, blue: 0.086, alpha: 1)], .white)
  default: return ([CGColor(red: 0.047, green: 0.047, blue: 0.047, alpha: 1)], .white)
  }
}

func symbol(_ px: Int) -> CGImage {
  let s = CGFloat(px)
  let ctx = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  ctx.interpolationQuality = .high
  ctx.setShouldAntialias(true)
  let inner = (s * 824 / 1024).rounded()
  let off = ((s - inner) / 2).rounded()
  let platte = CGRect(x: off, y: off, width: inner, height: inner)
  let pfad = CGPath(roundedRect: platte, cornerWidth: inner * 0.2237, cornerHeight: inner * 0.2237, transform: nil)
  // Schatten (nur ab mittleren Größen sichtbar sinnvoll).
  if px >= 64 {
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -s * 0.012), blur: s * 0.028, color: CGColor(gray: 0, alpha: 0.35))
    ctx.addPath(pfad); ctx.setFillColor(farben().flaeche[0]); ctx.fillPath()
    ctx.restoreGState()
  }
  ctx.saveGState()
  ctx.addPath(pfad); ctx.clip()
  let f = farben().flaeche
  if f.count > 1 {
    let verlauf = CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB)!, colors: f as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(verlauf, start: CGPoint(x: 0, y: off + inner), end: CGPoint(x: 0, y: off), options: [])
  } else {
    ctx.setFillColor(f[0]); ctx.fill(platte)
  }
  ctx.restoreGState()

  // Zeichen als Maske aus dem Vektor rendern (eigene Ebene), dann einfärben.
  let h = inner * anteil
  let faktor = h / zeichen.height
  let w = zeichen.width * faktor
  let ziel = CGRect(x: ((s - w) / 2), y: ((s - h) / 2), width: w, height: h)
  let maske = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
  maske.setFillColor(gray: 0, alpha: 1); maske.fill(CGRect(x: 0, y: 0, width: px, height: px))
  // Mindeststrichstärke: in kleinen Größen das Zeichen leicht verbreitert
  // zeichnen (mehrfach mit Versatz), damit Nadel und Ring nicht verschwimmen.
  let versatz = max(0, 0.55 - s * 0.0012)
  let schritte: [(CGFloat, CGFloat)] = versatz > 0.05
    ? [(0, 0), (versatz, 0), (-versatz, 0), (0, versatz), (0, -versatz)]
    : [(0, 0)]
  for (dx, dy) in schritte {
    maske.saveGState()
    maske.translateBy(x: ziel.minX + dx, y: ziel.minY + dy)
    maske.scaleBy(x: faktor, y: faktor)
    maske.translateBy(x: -zeichen.minX, y: -zeichen.minY)
    // Seite invertiert zeichnen: dunkles Zeichen → helle Maske.
    maske.setBlendMode(.normal)
    maske.drawPDFPage(seite)
    maske.restoreGState()
  }
  // Graustufen: Zeichen ist dunkel auf hell? Seite hat transparenten Grund —
  // wir haben schwarzen Grund vorbelegt und das Zeichen (dunkel) gezeichnet.
  // Deshalb als „dunkel = Zeichen“ neu bewerten: erneut mit Differenz-Ansatz.
  let leer = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
                       space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
  leer.setFillColor(gray: 1, alpha: 1); leer.fill(CGRect(x: 0, y: 0, width: px, height: px))
  for (dx, dy) in schritte {
    leer.saveGState()
    leer.translateBy(x: ziel.minX + dx, y: ziel.minY + dy)
    leer.scaleBy(x: faktor, y: faktor)
    leer.translateBy(x: -zeichen.minX, y: -zeichen.minY)
    leer.drawPDFPage(seite)
    leer.restoreGState()
  }
  // leer: weiß = Grund, dunkel = Zeichen → invertieren zur Maske (hell = Zeichen).
  let bpr = leer.bytesPerRow
  let p = leer.data!.bindMemory(to: UInt8.self, capacity: bpr * px)
  // Das Logo ist dunkelgrau (nicht schwarz) — auf volle Deckkraft strecken.
  var dunkelst: UInt8 = 255
  for i in 0..<(bpr * px) where p[i] < dunkelst { dunkelst = p[i] }
  let spanne = max(1, 255 - Int(dunkelst))
  for i in 0..<(bpr * px) { p[i] = UInt8(min(255, (255 - Int(p[i])) * 255 / spanne)) }
  let maskenBild = leer.makeImage()!
  ctx.saveGState()
  ctx.clip(to: CGRect(x: 0, y: 0, width: s, height: s), mask: maskenBild)
  ctx.setFillColor(farben().zeichen); ctx.fill(CGRect(x: 0, y: 0, width: s, height: s))
  ctx.restoreGState()
  _ = maske
  return ctx.makeImage()!
}

func speichern(_ bild: CGImage, _ name: String) {
  let rep = NSBitmapImageRep(cgImage: bild)
  try! rep.representation(using: .png, properties: [:])!.write(to: ordner.appendingPathComponent(name))
}

for (basis, fak) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)] {
  speichern(symbol(basis * fak), "icon_\(basis)x\(basis)\(fak == 2 ? "@2x" : "").png")
}
speichern(symbol(1024), "quelle-1024.png")
speichern(symbol(64), "vorschau-64.png")
print("fertig")
