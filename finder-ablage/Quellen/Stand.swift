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
}

final class Stand {
    static let wurzel = "root"
    private let url: URL
    private let sperre = NSLock()
    private var d: Datei
    private(set) var letzterAbgleich: Date = .distantPast

    init(domain: String) {
        let basis = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
            .appendingPathComponent("AtlasAblage", isDirectory: true)
        try? FileManager.default.createDirectory(at: basis, withIntermediateDirectories: true)
        let sicher = domain.replacingOccurrences(of: "/", with: "_")
        url = basis.appendingPathComponent("stand-\(sicher).json")
        if let daten = try? Data(contentsOf: url), let geladen = try? JSONDecoder().decode(Datei.self, from: daten) {
            d = geladen
        } else {
            d = Datei(kennung: UUID().uuidString, anker: 1, aeltesterAnker: 1, eintraege: [:], protokoll: [])
        }
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
        d.eintraege[e.id] = e
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
