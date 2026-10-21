// Ein Eintrag im Finder (NSFileProviderItem) aus dem gespeicherten Stand.

import FileProvider
import Foundation
import UniformTypeIdentifiers

final class Eintrag: NSObject, NSFileProviderItem {
    let g: Gespeichert?
    private let kennung: NSFileProviderItemIdentifier

    /// Oberste Ebene „Atlas“.
    static let wurzel = Eintrag(wurzel: ())

    private init(wurzel: ()) {
        g = nil
        kennung = .rootContainer
    }

    init(_ g: Gespeichert) {
        self.g = g
        kennung = NSFileProviderItemIdentifier(g.id)
    }

    var itemIdentifier: NSFileProviderItemIdentifier { kennung }

    var parentItemIdentifier: NSFileProviderItemIdentifier {
        guard let g, g.parent != Stand.wurzel else { return .rootContainer }
        return NSFileProviderItemIdentifier(g.parent)
    }

    var filename: String { g?.name ?? "Atlas" }

    var istOrdner: Bool { g == nil || g!.art == .workspace || g!.art == .ordner }

    var contentType: UTType {
        if istOrdner { return .folder }
        if let ext = g?.name.split(separator: ".").last.map(String.init), g!.name.contains("."),
           let t = UTType(filenameExtension: ext) { return t }
        if let m = g?.mime, let t = UTType(mimeType: m) { return t }
        return .data
    }

    var documentSize: NSNumber? {
        guard let s = g?.groesse, !istOrdner else { return nil }
        return NSNumber(value: s)
    }

    var creationDate: Date? { g?.erstellt }
    var contentModificationDate: Date? { g?.geaendert }

    var itemVersion: NSFileProviderItemVersion {
        guard let g else { return NSFileProviderItemVersion(contentVersion: Data("w".utf8), metadataVersion: Data("w".utf8)) }
        let rechte = "\(g.schreiben ? 1 : 0)\(g.loeschen ? 1 : 0)"
        return NSFileProviderItemVersion(contentVersion: Data(g.inhaltVersion.utf8),
                                         metadataVersion: Data((g.metaVersion + rechte).utf8))
    }

    var capabilities: NSFileProviderItemCapabilities {
        guard let g else { return [.allowsReading, .allowsContentEnumerating] }
        var c: NSFileProviderItemCapabilities = [.allowsReading]
        switch g.art {
        case .workspace:
            c.insert(.allowsContentEnumerating)
            if g.schreiben { c.insert(.allowsAddingSubItems) }
        case .ordner:
            c.insert(.allowsContentEnumerating)
            if g.schreiben { c.insert(.allowsAddingSubItems) }
        case .dokument:
            // Phase 1: Dateien aus Atlas sind auf dem Mac schreibgeschützt.
            if #unavailable(macOS 13) { c.insert(.allowsEvicting) }
        case .lokal:
            c.formUnion([.allowsWriting, .allowsRenaming, .allowsReparenting, .allowsDeleting])
        }
        return c
    }
}
