// „Ablage im Finder“: File-Provider-Erweiterung der Mac-App Sinclair Atlas.
//
// Finder → Atlas → <Workspace> → klassische Ablage (Ordnerbaum). Dateien sind
// Platzhalter und werden erst beim Öffnen geladen; macOS entfernt geladene
// Kopien bei Platzmangel wieder („Download entfernen“, „Immer behalten“
// funktionieren über das System). Neue Dateien und Ordner im Finder gehen
// nach Atlas; Änderungen in Atlas kommen über den Abgleich (Arbeitsmenge).
//
// Phase 1: Dateien aus Atlas sind auf dem Mac schreibgeschützt; Umbenennen,
// Verschieben und Löschen im Finder werden abgelehnt (das Original bleibt).

import FileProvider
import Foundation
import UniformTypeIdentifiers

/// Serialisiert Abgleiche (mehrere Aufzählungen gleichzeitig → ein Abruf).
actor Abgleicher {
    private var laufend: Task<Void, Error>?
    private var zuletzt: Date = .distantPast

    func abgleichen(maxAlter: TimeInterval, _ arbeit: @escaping () async throws -> Void) async throws {
        if Date().timeIntervalSince(zuletzt) < maxAlter { return }
        if let t = laufend { return try await t.value }
        let t = Task { try await arbeit() }
        laufend = t
        defer { laufend = nil }
        try await t.value
        zuletzt = Date()
    }
}

@objc(AtlasAblageErweiterung)
final class Erweiterung: NSObject, NSFileProviderReplicatedExtension {
    let domain: NSFileProviderDomain
    let stand: Stand
    let server = Server()
    let abgleicher = Abgleicher()

    required init(domain: NSFileProviderDomain) {
        self.domain = domain
        stand = Stand(domain: domain.identifier.rawValue)
        super.init()
    }

    func invalidate() {}

    var manager: NSFileProviderManager? { NSFileProviderManager(for: domain) }

    // MARK: Abgleich

    /// Holt den Serverstand und übernimmt ihn (höchstens alle `maxAlter` Sekunden).
    func abgleichen(maxAlter: TimeInterval = 10) async throws {
        try await abgleicher.abgleichen(maxAlter: maxAlter) { [self] in
            do {
                try await abgleichJetzt()
            } catch ServerFehler.widerrufen {
                // Zugang entzogen: alles entfernen, lokale Kopien verschwinden mit.
                ZugangSpeicher.vergessen()
                stand.allesEntfernen()
                throw ServerFehler.widerrufen
            }
        }
    }

    private func abgleichJetzt() async throws {
        let workspaces = try await server.workspaces()
        var neu: [String: Gespeichert] = [:]
        var vollstaendig = Set<String>()
        for ws in workspaces {
            let wid = "w:" + ws.id
            neu[wid] = Gespeichert(id: wid, parent: Stand.wurzel, name: ws.name, art: .workspace,
                                   workspaceId: ws.id, groesse: nil, mime: nil, version: 1,
                                   inhaltVersion: "w", metaVersion: ws.name, erstellt: nil, geaendert: nil,
                                   schreiben: ws.schreiben, loeschen: ws.loeschen)
            do {
                let l = try await server.liste(ws.id)
                for e in l.eintraege {
                    let g = Self.alsGespeichert(e, workspaceId: ws.id, schreiben: l.schreiben, loeschen: l.loeschen)
                    neu[g.id] = g
                }
                vollstaendig.insert(ws.id)
            } catch ServerFehler.keinRecht {
                // Recht inzwischen entzogen: Inhalt gilt als vollständig leer.
                vollstaendig.insert(ws.id)
            } catch ServerFehler.netz {
                // Workspace diesmal nicht erreichbar: alten Inhalt behalten.
                for e in stand.alle() where e.workspaceId == ws.id && e.art != .workspace { neu[e.id] = e }
            }
        }
        // Workspace-Namen eindeutig halten (zwei Kunden mit gleichem Namen).
        stand.uebernehmen(neu, vollstaendig: vollstaendig, workspacesVollstaendig: true)
    }

    static func alsGespeichert(_ e: ServerEintrag, workspaceId: String, schreiben: Bool, loeschen: Bool) -> Gespeichert {
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let iso2 = ISO8601DateFormatter()
        func datum(_ s: String) -> Date? { iso.date(from: s) ?? iso2.date(from: s) }
        let parent = e.parentId.map { "o:" + $0 } ?? ("w:" + workspaceId)
        return Gespeichert(id: (e.art == "ordner" ? "o:" : "d:") + e.id, parent: parent, name: e.name,
                           art: e.art == "ordner" ? .ordner : .dokument, workspaceId: workspaceId,
                           groesse: e.groesse, mime: e.mime, version: e.version,
                           inhaltVersion: e.inhaltVersion, metaVersion: e.metaVersion,
                           erstellt: datum(e.erstelltAm), geaendert: datum(e.geaendertAm),
                           schreiben: schreiben, loeschen: loeschen)
    }

    func fehler(_ e: Error) -> Error {
        (e as? ServerFehler)?.alsFileProviderFehler ?? e
    }

    // MARK: Einträge

    func item(for identifier: NSFileProviderItemIdentifier, request: NSFileProviderRequest,
              completionHandler: @escaping (NSFileProviderItem?, Error?) -> Void) -> Progress
    {
        if identifier == .rootContainer {
            completionHandler(Eintrag.wurzel, nil)
        } else if let g = stand.eintrag(identifier.rawValue) {
            completionHandler(Eintrag(g), nil)
        } else {
            completionHandler(nil, NSError.fileProviderErrorForNonExistentItem(withIdentifier: identifier))
        }
        return Progress()
    }

    func fetchContents(for itemIdentifier: NSFileProviderItemIdentifier, version requestedVersion: NSFileProviderItemVersion?,
                       request: NSFileProviderRequest,
                       completionHandler: @escaping (URL?, NSFileProviderItem?, Error?) -> Void) -> Progress
    {
        let fortschritt = Progress(totalUnitCount: 100)
        guard let g = stand.eintrag(itemIdentifier.rawValue), g.art == .dokument else {
            completionHandler(nil, nil, NSError.fileProviderErrorForNonExistentItem(withIdentifier: itemIdentifier))
            return fortschritt
        }
        let task = Task {
            do {
                let tmpOrdner = try manager?.temporaryDirectoryURL() ?? FileManager.default.temporaryDirectory
                let ziel = tmpOrdner.appendingPathComponent(UUID().uuidString)
                try await server.laden(g.atlasId, nach: ziel, fortschritt: fortschritt)
                // Groesse aus der Datei übernehmen (ältere Dokumente haben keine).
                var aktuell = stand.eintrag(g.id) ?? g
                if let s = (try? FileManager.default.attributesOfItem(atPath: ziel.path)[.size]) as? NSNumber,
                   aktuell.groesse != s.int64Value
                {
                    aktuell.groesse = s.int64Value
                    stand.setzen(aktuell)
                }
                completionHandler(ziel, Eintrag(aktuell), nil)
            } catch {
                completionHandler(nil, nil, fehler(error))
            }
        }
        fortschritt.cancellationHandler = { task.cancel() }
        return fortschritt
    }

    // MARK: Anlegen (Finder → Atlas)

    /// Wohin gehört ein neuer Eintrag? (Workspace-ID, Atlas-Ordner-ID oder nil)
    func ziel(_ parent: NSFileProviderItemIdentifier) -> (workspace: String, ordner: String?, schreiben: Bool)? {
        guard parent != .rootContainer, let p = stand.eintrag(parent.rawValue) else { return nil }
        switch p.art {
        case .workspace: return (p.workspaceId, nil, p.schreiben)
        case .ordner: return (p.workspaceId, p.atlasId, p.schreiben)
        default: return nil
        }
    }

    func createItem(basedOn itemTemplate: NSFileProviderItem, fields: NSFileProviderItemFields, contents url: URL?,
                    options: NSFileProviderCreateItemOptions = [], request: NSFileProviderRequest,
                    completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void) -> Progress
    {
        let fortschritt = Progress(totalUnitCount: 100)
        let name = itemTemplate.filename
        let istOrdner = itemTemplate.contentType == .folder || itemTemplate.contentType?.conforms(to: .folder) == true

        // Bei erneutem Import (mayAlreadyExist) vorhandenen Eintrag gleichen Namens übernehmen.
        if options.contains(.mayAlreadyExist),
           let vorhanden = stand.kinder(von: itemTemplate.parentItemIdentifier.rawValue)
               .first(where: { $0.name.lowercased() == name.lowercased() })
        {
            completionHandler(Eintrag(vorhanden), [], !istOrdner, nil)
            return fortschritt
        }

        // Hilfsdateien (.DS_Store, ~$…, *.tmp) bleiben nur auf diesem Mac.
        if Hilfsdatei.ist(name) || itemTemplate.contentType == .symbolicLink || itemTemplate.contentType == .aliasFile {
            let lokal = Gespeichert(id: "l:" + UUID().uuidString, parent: itemTemplate.parentItemIdentifier == .rootContainer
                ? Stand.wurzel : itemTemplate.parentItemIdentifier.rawValue,
                name: name, art: .lokal, workspaceId: ziel(itemTemplate.parentItemIdentifier)?.workspace ?? "",
                groesse: itemTemplate.documentSize??.int64Value, mime: nil, version: 1, inhaltVersion: UUID().uuidString,
                metaVersion: "l", erstellt: Date(), geaendert: Date(), schreiben: true, loeschen: true)
            stand.setzen(lokal)
            completionHandler(Eintrag(lokal), [], false, nil)
            return fortschritt
        }

        guard let z = ziel(itemTemplate.parentItemIdentifier) else {
            completionHandler(nil, [], false, NSError(domain: NSCocoaErrorDomain, code: NSFileWriteNoPermissionError, userInfo: [
                NSLocalizedDescriptionKey: "Hier können keine Dateien abgelegt werden. Bitte in einen Workspace-Ordner legen.",
            ]))
            return fortschritt
        }
        guard z.schreiben else {
            completionHandler(nil, [], false, ServerFehler.keinRecht("Keine Schreibrechte in dieser Ablage.").alsFileProviderFehler)
            return fortschritt
        }

        let task = Task {
            do {
                let e: ServerEintrag
                if istOrdner {
                    e = try await server.ordnerAnlegen(workspaceId: z.workspace, ordnerId: z.ordner, name: name)
                } else {
                    guard let url else { throw ServerFehler.abgelehnt("Kein Inhalt.") }
                    let mime = itemTemplate.contentType?.preferredMIMEType ?? "application/octet-stream"
                    e = try await server.hochladen(workspaceId: z.workspace, ordnerId: z.ordner, name: name, mime: mime, datei: url)
                }
                let p = stand.eintrag(itemTemplate.parentItemIdentifier.rawValue)
                let g = Self.alsGespeichert(e, workspaceId: z.workspace, schreiben: p?.schreiben ?? z.schreiben,
                                            loeschen: p?.loeschen ?? false)
                stand.setzen(g)
                fortschritt.completedUnitCount = 100
                completionHandler(Eintrag(g), [], false, nil)
            } catch {
                completionHandler(nil, [], false, fehler(error))
            }
        }
        fortschritt.cancellationHandler = { task.cancel() }
        return fortschritt
    }

    // MARK: Ändern / Löschen

    func modifyItem(_ item: NSFileProviderItem, baseVersion version: NSFileProviderItemVersion,
                    changedFields: NSFileProviderItemFields, contents newContents: URL?,
                    options: NSFileProviderModifyItemOptions = [], request: NSFileProviderRequest,
                    completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void) -> Progress
    {
        let id = item.itemIdentifier.rawValue
        guard let g = stand.eintrag(id) else {
            completionHandler(nil, [], false, NSError.fileProviderErrorForNonExistentItem(withIdentifier: item.itemIdentifier))
            return Progress()
        }
        if g.art == .lokal {
            var neu = g
            neu.name = item.filename
            neu.parent = item.parentItemIdentifier == .rootContainer ? Stand.wurzel : item.parentItemIdentifier.rawValue
            if changedFields.contains(.contents) { neu.inhaltVersion = UUID().uuidString }
            stand.setzen(neu)
            completionHandler(Eintrag(neu), [], false, nil)
            return Progress()
        }
        // Nur Finder-eigene Merkmale (Tags, Zuletzt benutzt, Favoriten) — die bleiben lokal.
        let erlaubt: NSFileProviderItemFields = [.lastUsedDate, .tagData, .favoriteRank, .fileSystemFlags, .extendedAttributes,
                                                 .creationDate, .contentModificationDate]
        if changedFields.isSubset(of: erlaubt) {
            completionHandler(Eintrag(g), [], false, nil)
            return Progress()
        }
        // Phase 1: Inhalt, Name und Ort ändern geht nur in Atlas. Original bleibt, die
        // lokale Änderung wird zurückgesetzt (Inhalt neu laden).
        completionHandler(Eintrag(g), [], changedFields.contains(.contents), nil)
        return Progress()
    }

    func deleteItem(identifier: NSFileProviderItemIdentifier, baseVersion version: NSFileProviderItemVersion,
                    options: NSFileProviderDeleteItemOptions = [], request: NSFileProviderRequest,
                    completionHandler: @escaping (Error?) -> Void) -> Progress
    {
        guard let g = stand.eintrag(identifier.rawValue) else {
            completionHandler(nil)
            return Progress()
        }
        if g.art == .lokal {
            stand.entfernen(g.id)
            completionHandler(nil)
            return Progress()
        }
        // Phase 1: Löschen nur in Atlas (dort Papierkorb). Der Finder stellt den Eintrag wieder her.
        completionHandler(NSFileProviderError(.deletionRejected))
        return Progress()
    }

    // MARK: Aufzählen

    func enumerator(for containerItemIdentifier: NSFileProviderItemIdentifier, request: NSFileProviderRequest) throws -> NSFileProviderEnumerator {
        if containerItemIdentifier == .trashContainer { return LeererAufzaehler(stand: stand) }
        if containerItemIdentifier != .rootContainer, containerItemIdentifier != .workingSet,
           stand.eintrag(containerItemIdentifier.rawValue) == nil
        {
            throw NSError.fileProviderErrorForNonExistentItem(withIdentifier: containerItemIdentifier)
        }
        return Aufzaehler(container: containerItemIdentifier, erweiterung: self)
    }
}

enum Hilfsdatei {
    /// Dateien, die Programme nur zwischenspeichern (gleiche Regel wie istHilfsdatei in Atlas).
    static func ist(_ name: String) -> Bool {
        let n = name.trimmingCharacters(in: .whitespaces)
        if n.isEmpty || n.hasPrefix(".") || n.hasPrefix("~$") { return true }
        let klein = n.lowercased()
        for e in [".tmp", ".temp", ".crdownload", ".part", ".download"] where klein.hasSuffix(e) { return true }
        if klein.range(of: #"\.sb-[a-z0-9]+$"#, options: .regularExpression) != nil { return true }
        if ["thumbs.db", "desktop.ini", "icon\r"].contains(klein) { return true }
        return false
    }
}
