// Letzter bekannter Stand der Ablage auf diesem Mac + Änderungsprotokoll.
//
// Atlas liefert je Workspace die vollständige Liste (Ordner + Dokumente).
// Der Abgleich vergleicht sie mit dem letzten Stand: neue oder geänderte
// Einträge → „aktualisiert“, verschwundene → „gelöscht“ (auch wenn nur das
// Recht entzogen wurde — der Finder entfernt dann die lokale Kopie). Jede
// Änderungsrunde bekommt einen neuen Anker; der Finder fragt mit seinem
// letzten Anker nach, was seitdem passiert ist.
//
// Gespeichert im eigenen (abgeschotteten) Container der Erweiterung, nur
// Namen und Versionen — keine Dateiinhalte.

import Foundation

/// Ein Eintrag, wie ihn die Erweiterung kennt.
struct Gespeichert: Codable, Equatable {
    enum Art: String, Codable { case workspace, ordner, dokument, lokal }
    var id: String // Kennung im Finder: w:/o:/d:/l: + UUID
    var parent: String // Kennung des Elternteils ("root" = oberste Ebene)
    var name: String
    var art: Art
    var workspaceId: String
    var groesse: Int64?
    var mime: String?
    var version: Int
    var inhaltVersion: String
    var metaVersion: String
    var erstellt: Date?
    var geaendert: Date?
    var schreiben: Bool
    var loeschen: Bool
    /// Nur lesbar (signierte Fassung). Optional, damit ältere Stände lesbar bleiben.
    var nurLesen: Bool? = nil
    /// "workspace" | "universe_intern"
    var sichtbarkeit: String? = nil
    /// Darf die Person die Sichtbarkeit umstellen?
    var intern: Bool? = nil

    /// UUID in Atlas (ohne Präfix).
    var atlasId: String { String(id.dropFirst(2)) }
}

struct Protokoll: Codable {
    var anker: Int
    var id: String
    var geloescht: Bool
}

private struct Datei: Codable {
    var kennung: String
    var anker: Int
    var aeltesterAnker: Int
    var eintraege: [String: Gespeichert]
    var protokoll: [Protokoll]
    /// Inhalts-Epoche (siehe Stand.epoche). Fehlt bei Ständen bis 0.3.0-beta.4.
    var epoche: Int?
    /// Workspaces, die im Finder schon geöffnet wurden — nur deren Inhalt wird abgeglichen.
    var geoeffnet: [String]?
}

final class Stand {
    static let wurzel = "root"
    private let url: URL
    private let sperre = NSLock()
    private var d: Datei
    private(set) var letzterAbgleich: Date = .distantPast

    var istLeer: Bool {
        sperre.lock(); defer { sperre.unlock() }
        return d.eintraege.isEmpty
    }

    init(domain: String) {
        let basis = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
            .appendingPathComponent("AtlasAblage", isDirectory: true)
        try? FileManager.default.createDirectory(at: basis, withIntermediateDirectories: true)
        let sicher = domain.replacingOccurrences(of: "/", with: "_")
        url = basis.appendingPathComponent("stand-\(sicher).json")
        if let daten = try? Data(contentsOf: url), let geladen = try? JSONDecoder().decode(Datei.self, from: daten) {
            d = geladen
        } else {
            d = Datei(kennung: UUID().uuidString, anker: 1, aeltesterAnker: 1, eintraege: [:], protokoll: [],
                      epoche: Stand.epoche, geoeffnet: [])
        }
        if d.geoeffnet == nil {
            // Ältere Stände: alle bekannten Workspaces gelten als geöffnet.
            d.geoeffnet = Array(Set(d.eintraege.values.filter { $0.art != .workspace }.map(\.workspaceId)))
        }
        if d.epoche != Stand.epoche {
            // Neue Epoche: allen Einträgen eine neue Inhaltsversion melden, damit
            // macOS früher (leer) geladene Inhalte verwirft und neu lädt.
            epocheGewechselt = !d.eintraege.isEmpty
            let naechster = d.anker + 1
            for id in d.eintraege.keys { d.protokoll.append(Protokoll(anker: naechster, id: id, geloescht: false)) }
            d.anker = naechster
            d.epoche = Stand.epoche
            speichern()
            protokoll.notice("Stand: neue Inhalts-Epoche \(Stand.epoche), \(self.d.eintraege.count) Einträge neu gemeldet")
        }
    }

    /// Erhöhen, wenn alle Inhalte auf den Rechnern neu geladen werden müssen
    /// (2 = 0.3.0-beta.5: in beta.1 als leer geladene Dateien).
    static let epoche = 2
    /// Wurde die Epoche beim Start gewechselt? (Dann werden Dokumente geräumt.)
    private(set) var epocheGewechselt = false

    func geoeffneteWorkspaces() -> [String] {
        sperre.lock(); defer { sperre.unlock() }
        return d.geoeffnet ?? []
    }

    /// Workspace als geöffnet merken. true = war neu.
    func oeffnen(_ ws: String) -> Bool {
        sperre.lock(); defer { sperre.unlock() }
        if d.geoeffnet?.contains(ws) == true { return false }
        d.geoeffnet = (d.geoeffnet ?? []) + [ws]
        speichern()
        return true
    }

    private func speichern() {
        if let daten = try? JSONEncoder().encode(d) {
            try? daten.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        }
    }

    // MARK: Anker

    func ankerDaten() -> Data {
        sperre.lock(); defer { sperre.unlock() }
        return Data("\(d.kennung):\(d.anker)".utf8)
    }

    /// nil = Anker unbekannt oder zu alt (Finder muss neu aufzählen).
    private func ankerWert(_ daten: Data) -> Int? {
        guard let s = String(data: daten, encoding: .utf8) else { return nil }
        let teile = s.split(separator: ":")
        guard teile.count == 2, String(teile[0]) == d.kennung, let n = Int(teile[1]) else { return nil }
        return n
    }

    // MARK: Lesen

    func eintrag(_ id: String) -> Gespeichert? {
        sperre.lock(); defer { sperre.unlock() }
        return d.eintraege[id]
    }

    func kinder(von parent: String) -> [Gespeichert] {
        sperre.lock(); defer { sperre.unlock() }
        return d.eintraege.values.filter { $0.parent == parent }.sorted { $0.id < $1.id }
    }

    func alle() -> [Gespeichert] {
        sperre.lock(); defer { sperre.unlock() }
        return d.eintraege.values.sorted { $0.id < $1.id }
    }

    /// Änderungen seit `anker`: (aktualisiert, gelöscht, neuer Anker) oder nil, wenn der Anker abgelaufen ist.
    func aenderungen(seit anker: Data) -> (aktualisiert: [Gespeichert], geloescht: [String], anker: Data)? {
        sperre.lock(); defer { sperre.unlock() }
        guard let von = ankerWert(anker), von >= d.aeltesterAnker - 1, von <= d.anker else { return nil }
        var letzte: [String: Bool] = [:]
        for p in d.protokoll where p.anker > von { letzte[p.id] = p.geloescht }
        var aktualisiert: [Gespeichert] = []
        var geloescht: [String] = []
        for (id, weg) in letzte {
            if !weg, let e = d.eintraege[id] { aktualisiert.append(e) } else if d.eintraege[id] == nil { geloescht.append(id) }
        }
        return (aktualisiert, geloescht, Data("\(d.kennung):\(d.anker)".utf8))
    }

    // MARK: Schreiben

    /// Neuen Serverstand übernehmen. `vollstaendig`: Workspaces, deren Liste
    /// erfolgreich geladen wurde — nur dort darf gelöscht werden.
    /// Liefert true, wenn sich etwas geändert hat.
    @discardableResult
    func uebernehmen(_ neu: [String: Gespeichert], vollstaendig: Set<String>, workspacesVollstaendig: Bool) -> Bool {
        sperre.lock(); defer { sperre.unlock() }
        var aenderungen: [Protokoll] = []
        let naechster = d.anker + 1
        var ergebnis = d.eintraege
        for (id, e) in neu where ergebnis[id] != e {
            ergebnis[id] = e
            aenderungen.append(Protokoll(anker: naechster, id: id, geloescht: false))
        }
        for (id, alt) in d.eintraege where neu[id] == nil {
            let darfWeg: Bool
            switch alt.art {
            case .lokal: darfWeg = false
            case .workspace: darfWeg = workspacesVollstaendig
            case .ordner, .dokument: darfWeg = vollstaendig.contains(alt.workspaceId)
                || (workspacesVollstaendig && neu["w:" + alt.workspaceId] == nil)
            }
            if darfWeg {
                ergebnis.removeValue(forKey: id)
                aenderungen.append(Protokoll(anker: naechster, id: id, geloescht: true))
            }
        }
        // Lokale Hilfsdateien, deren Ordner verschwunden ist, mit entfernen.
        for (id, e) in ergebnis where e.art == .lokal && e.parent != Stand.wurzel && ergebnis[e.parent] == nil {
            ergebnis.removeValue(forKey: id)
            aenderungen.append(Protokoll(anker: naechster, id: id, geloescht: true))
        }
        letzterAbgleich = Date()
        guard !aenderungen.isEmpty else { return false }
        d.eintraege = ergebnis
        d.anker = naechster
        d.protokoll.append(contentsOf: aenderungen)
        // Protokoll begrenzen: ältere Anker gelten dann als abgelaufen.
        if d.protokoll.count > 20_000 {
            let rest = d.protokoll.suffix(10_000)
            d.protokoll = Array(rest)
            d.aeltesterAnker = (rest.first?.anker ?? naechster)
        }
        speichern()
        return true
    }

    /// Einzelnen Eintrag setzen (nach Anlegen/Ändern über den Finder) — ohne
    /// Protokoll, der Finder kennt den neuen Stand schon.
    func setzen(_ e: Gespeichert) {
        sperre.lock(); defer { sperre.unlock() }
        d.eintraege[e.id] = mitIntern(e)
        speichern()
    }

    /// Eintrag setzen und dem Finder melden (z. B. Konfliktkopie, die der
    /// Server neu angelegt hat).
    func setzenUndMelden(_ e: Gespeichert) {
        sperre.lock(); defer { sperre.unlock() }
        d.eintraege[e.id] = mitIntern(e)
        d.anker += 1
        d.protokoll.append(Protokoll(anker: d.anker, id: e.id, geloescht: false))
        speichern()
    }

    /// „intern“ vom bisherigen Eintrag bzw. Elternordner übernehmen (nicht jede Antwort trägt es).
    private func mitIntern(_ e: Gespeichert) -> Gespeichert {
        guard e.intern == nil else { return e }
        var n = e
        n.intern = d.eintraege[e.id]?.intern ?? d.eintraege[e.parent]?.intern
        return n
    }

    /// Eintrag samt allem darunter entfernen (der Finder hat sie selbst gelöscht).
    func entfernenMitInhalt(_ id: String) {
        sperre.lock(); defer { sperre.unlock() }
        var weg: Set<String> = [id]
        var gewachsen = true
        while gewachsen {
            gewachsen = false
            for (k, e) in d.eintraege where !weg.contains(k) && weg.contains(e.parent) {
                weg.insert(k)
                gewachsen = true
            }
        }
        for k in weg { d.eintraege.removeValue(forKey: k) }
        speichern()
    }

    func entfernen(_ id: String) {
        sperre.lock(); defer { sperre.unlock() }
        d.eintraege.removeValue(forKey: id)
        speichern()
    }

    /// Zugang widerrufen: alles weg (der Finder entfernt die lokalen Kopien).
    func allesEntfernen() {
        sperre.lock(); defer { sperre.unlock() }
        guard !d.eintraege.isEmpty else { return }
        let naechster = d.anker + 1
        for id in d.eintraege.keys { d.protokoll.append(Protokoll(anker: naechster, id: id, geloescht: true)) }
        d.eintraege = [:]
        d.anker = naechster
        speichern()
    }
}
