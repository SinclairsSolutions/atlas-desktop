// „Ablage im Finder“: File-Provider-Erweiterung der Mac-App Sinclair Atlas.
//
// Finder → Atlas → <Workspace> → klassische Ablage (Ordnerbaum). Dateien sind
// Platzhalter und werden erst beim Öffnen geladen; macOS entfernt geladene
// Kopien bei Platzmangel wieder („Download entfernen“, „Immer behalten“
// funktionieren über das System). Neue Dateien und Ordner im Finder gehen
// nach Atlas; Änderungen in Atlas kommen über den Abgleich (Arbeitsmenge).
//
// Phase 2: Bearbeiten speichert eine neue Version (die alte bleibt in Atlas);
// hat sich das Dokument in Atlas inzwischen geändert, bleibt es unverändert und
// die Fassung vom Mac kommt als Konfliktkopie daneben. Umbenennen und
// Verschieben (innerhalb eines Workspace) gehen durch; Löschen legt in den
// Atlas-Papierkorb (nur mit Verwalten-Recht, sonst stellt der Finder es wieder her).

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
        let start = Date()
        var neu: [String: Gespeichert] = [:]
        var vollstaendig = Set<String>()
        func workspaceEintrag(_ ws: WorkspaceInfo) {
            let wid = "w:" + ws.id
            neu[wid] = Gespeichert(id: wid, parent: Stand.wurzel, name: ws.name, art: .workspace,
                                   workspaceId: ws.id, groesse: nil, mime: nil, version: 1,
                                   inhaltVersion: "w", metaVersion: ws.name, erstellt: nil, geaendert: nil,
                                   schreiben: ws.schreiben, loeschen: ws.loeschen)
        }
        func uebernehmen(_ l: ServerListe, _ ws: WorkspaceInfo) {
            for e in l.eintraege {
                let g = Self.alsGespeichert(e, workspaceId: ws.id, schreiben: l.schreiben, loeschen: l.loeschen)
                neu[g.id] = g
            }
            vollstaendig.insert(ws.id)
        }
        func altBehalten(_ ws: WorkspaceInfo) {
            for e in stand.alle() where e.workspaceId == ws.id && e.art != .workspace { neu[e.id] = e }
        }

        if let alles = try await server.alles() {
            // Eine Anfrage; der Server lädt die Workspaces parallel.
            for ws in alles.workspaces {
                workspaceEintrag(ws)
                switch alles.listen[ws.id] {
                case .liste(let l)?: uebernehmen(l, ws)
                case .fehler("kein_recht")?: vollstaendig.insert(ws.id)
                default: altBehalten(ws)
                }
            }
        } else {
            // Älterer Server ohne /alles: Listen parallel abrufen.
            let workspaces = try await server.workspaces()
            for ws in workspaces { workspaceEintrag(ws) }
            let ergebnisse = await withTaskGroup(of: (String, Result<ServerListe, Error>).self) { gruppe in
                for ws in workspaces {
                    gruppe.addTask { [server] in
                        do { return (ws.id, .success(try await server.liste(ws.id))) } catch { return (ws.id, .failure(error)) }
                    }
                }
                var r: [String: Result<ServerListe, Error>] = [:]
                for await (id, e) in gruppe { r[id] = e }
                return r
            }
            for ws in workspaces {
                switch ergebnisse[ws.id] {
                case .success(let l)?: uebernehmen(l, ws)
                case .failure(ServerFehler.keinRecht)?: vollstaendig.insert(ws.id)
                case .failure(ServerFehler.widerrufen)?: throw ServerFehler.widerrufen
                default: altBehalten(ws)
                }
            }
        }
        let geaendert = stand.uebernehmen(neu, vollstaendig: vollstaendig, workspacesVollstaendig: true)
        protokoll.info("Abgleich: \(neu.count) Einträge, geändert \(geaendert) in \(Date().timeIntervalSince(start)) s")
    }

    /// Abgleich im Hintergrund; meldet Änderungen danach dem Finder.
    func abgleichImHintergrund() {
        Task { [weak self] in
            guard let self else { return }
            do {
                try await self.abgleichen(maxAlter: 15)
                self.manager?.signalEnumerator(for: .workingSet) { _ in }
            } catch {
                protokoll.error("Abgleich: \(String(describing: error), privacy: .public)")
            }
        }
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
                           schreiben: schreiben, loeschen: loeschen, nurLesen: e.nurLesen)
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
                let groesse = try await server.laden(g.atlasId, nach: ziel, fortschritt: fortschritt)
                // Größe aus der Datei übernehmen (ältere Dokumente haben keine).
                var aktuell = stand.eintrag(g.id) ?? g
                if aktuell.groesse != groesse {
                    aktuell.groesse = groesse
                    stand.setzen(aktuell)
                }
                completionHandler(ziel, Eintrag(aktuell), nil)
            } catch {
                protokoll.error("Öffnen \(g.name, privacy: .public): \(String(describing: error), privacy: .public)")
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

    // MARK: Ändern / Löschen (Phase 2)

    /// Atlas-Ordner-ID für einen neuen Elternteil im selben Workspace.
    /// nil = nicht zulässig (anderer Workspace, oberste Ebene „Atlas“).
    private func ordnerZiel(_ parent: NSFileProviderItemIdentifier, workspace: String) -> String?? {
        guard let z = ziel(parent), z.workspace == workspace else { return nil }
        return .some(z.ordner)
    }

    private func zuruecksetzen(_ g: Gespeichert, inhalt: Bool,
                               _ completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void)
    {
        // Der Finder übernimmt den Serverstand wieder (Name, Ort; bei Inhalt: neu laden).
        completionHandler(Eintrag(g), [], inhalt, nil)
    }

    func modifyItem(_ item: NSFileProviderItem, baseVersion version: NSFileProviderItemVersion,
                    changedFields: NSFileProviderItemFields, contents newContents: URL?,
                    options: NSFileProviderModifyItemOptions = [], request: NSFileProviderRequest,
                    completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void) -> Progress
    {
        let fortschritt = Progress(totalUnitCount: 100)
        let id = item.itemIdentifier.rawValue
        guard let g = stand.eintrag(id) else {
            completionHandler(nil, [], false, NSError.fileProviderErrorForNonExistentItem(withIdentifier: item.itemIdentifier))
            return fortschritt
        }
        if g.art == .lokal {
            var neu = g
            neu.name = item.filename
            neu.parent = item.parentItemIdentifier == .rootContainer ? Stand.wurzel : item.parentItemIdentifier.rawValue
            if changedFields.contains(.contents) { neu.inhaltVersion = UUID().uuidString }
            stand.setzen(neu)
            completionHandler(Eintrag(neu), [], false, nil)
            return fortschritt
        }
        let umbenennen = changedFields.contains(.filename) && item.filename != g.name
        let verschieben = changedFields.contains(.parentItemIdentifier) && item.parentItemIdentifier.rawValue != g.parent
        let inhalt = changedFields.contains(.contents) && g.art == .dokument
        // Nur Finder-eigene Merkmale (Tags, Zuletzt benutzt …) — bleiben lokal.
        if !umbenennen && !verschieben && !inhalt {
            completionHandler(Eintrag(g), [], false, nil)
            return fortschritt
        }
        // Workspaces selbst lassen sich nicht ändern; ohne Schreibrecht nichts.
        if g.art == .workspace || !g.schreiben {
            zuruecksetzen(g, inhalt: inhalt, completionHandler)
            return fortschritt
        }
        var ordnerId: String?? = nil
        if verschieben {
            guard let o = ordnerZiel(item.parentItemIdentifier, workspace: g.workspaceId) else {
                // Zwischen Workspaces verschieben geht nur in Atlas.
                zuruecksetzen(g, inhalt: inhalt, completionHandler)
                return fortschritt
            }
            ordnerId = o
        }
        let basis = String(data: version.contentVersion, encoding: .utf8) ?? g.inhaltVersion
        let mime = item.contentType?.preferredMIMEType ?? g.mime ?? "application/octet-stream"
        let task = Task {
            do {
                var aktuell = g
                if umbenennen || verschieben {
                    let e = try await server.aendern(id: g.atlasId, name: umbenennen ? item.filename : nil, ordnerId: ordnerId)
                    aktuell = Self.alsGespeichert(e, workspaceId: g.workspaceId, schreiben: g.schreiben, loeschen: g.loeschen)
                }
                var nachladen = false
                if inhalt, let datei = newContents {
                    let r = try await server.neueVersion(dokumentId: g.atlasId, basis: basis, mime: mime, datei: datei)
                    aktuell = Self.alsGespeichert(r.eintrag, workspaceId: g.workspaceId, schreiben: g.schreiben, loeschen: g.loeschen)
                    if let k = r.konflikt {
                        // Atlas hatte inzwischen eine neuere Fassung: die bleibt, die Änderung
                        // vom Mac liegt als Konfliktkopie daneben.
                        let kopie = Self.alsGespeichert(k, workspaceId: g.workspaceId, schreiben: g.schreiben, loeschen: g.loeschen)
                        stand.setzenUndMelden(kopie)
                        nachladen = true
                        manager?.signalEnumerator(for: .workingSet) { _ in }
                    }
                }
                stand.setzen(aktuell)
                fortschritt.completedUnitCount = 100
                completionHandler(Eintrag(aktuell), [], nachladen, nil)
            } catch ServerFehler.keinRecht, ServerFehler.nichtGefunden, ServerFehler.abgelehnt {
                // Abgelehnt: Serverstand gilt (bei Inhalt neu laden).
                zuruecksetzen(stand.eintrag(id) ?? g, inhalt: inhalt, completionHandler)
            } catch {
                completionHandler(nil, [], false, fehler(error))
            }
        }
        fortschritt.cancellationHandler = { task.cancel() }
        return fortschritt
    }

    func deleteItem(identifier: NSFileProviderItemIdentifier, baseVersion version: NSFileProviderItemVersion,
                    options: NSFileProviderDeleteItemOptions = [], request: NSFileProviderRequest,
                    completionHandler: @escaping (Error?) -> Void) -> Progress
    {
        let fortschritt = Progress(totalUnitCount: 1)
        guard let g = stand.eintrag(identifier.rawValue) else {
            completionHandler(nil)
            return fortschritt
        }
        if g.art == .lokal {
            stand.entfernen(g.id)
            completionHandler(nil)
            return fortschritt
        }
        // Nie endgültig: Löschen im Finder legt den Eintrag in den Atlas-Papierkorb.
        // Ohne Verwalten-Recht (oder bei Workspaces) stellt der Finder ihn wieder her.
        if g.art == .workspace || !g.loeschen {
            completionHandler(NSFileProviderError(.deletionRejected))
            return fortschritt
        }
        let task = Task {
            do {
                try await server.loeschen(id: g.atlasId)
                stand.entfernenMitInhalt(g.id)
                completionHandler(nil)
            } catch ServerFehler.nichtGefunden {
                stand.entfernenMitInhalt(g.id)
                completionHandler(nil)
            } catch ServerFehler.keinRecht, ServerFehler.abgelehnt {
                completionHandler(NSFileProviderError(.deletionRejected))
            } catch {
                completionHandler(fehler(error))
            }
        }
        fortschritt.cancellationHandler = { task.cancel() }
        return fortschritt
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
