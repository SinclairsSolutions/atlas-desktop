// Verbindung zu Atlas: ausschließlich /api/public/desktop/ablage/* auf dem
// Atlas-Server (keine Dritten). Ephemere Sitzung: kein Cache, keine Cookies.

import FileProvider
import Foundation
import os

/// Protokoll der Erweiterung (Konsole.app → Prozess „AtlasAblage“), ohne Inhalte.
let protokoll = Logger(subsystem: "com.sinclairs-solutions.atlas.ablage", category: "ablage")

struct WorkspaceInfo: Codable {
    let id: String
    let name: String
    let schreiben: Bool
    let loeschen: Bool
}

struct ServerEintrag: Codable {
    let id: String
    let art: String // "ordner" | "dokument"
    let name: String
    let parentId: String?
    let groesse: Int64?
    let mime: String?
    let version: Int
    let inhaltVersion: String
    let metaVersion: String
    let erstelltAm: String
    let geaendertAm: String
    /// Nur lesbar (z. B. signierte Fassung).
    let nurLesen: Bool?
}

struct ServerListe: Codable {
    let workspaceId: String
    let schreiben: Bool
    let loeschen: Bool
    let fingerabdruck: String
    let eintraege: [ServerEintrag]
}

struct VersionErgebnis: Codable {
    let eintrag: ServerEintrag
    let konflikt: ServerEintrag?
}

private struct WorkspacesAntwort: Codable { let workspaces: [WorkspaceInfo] }

/// Antwort von /alles: Workspaces und je Workspace die Liste oder ein Fehler.
struct AllesAntwort: Decodable {
    let workspaces: [WorkspaceInfo]
    let listen: [String: ListeOderFehler]
}

enum ListeOderFehler: Decodable {
    case liste(ServerListe)
    case fehler(String)

    init(from decoder: Decoder) throws {
        if let l = try? ServerListe(from: decoder) {
            self = .liste(l)
            return
        }
        struct F: Decodable { let fehler: String }
        self = .fehler((try? F(from: decoder).fehler) ?? "abfrage")
    }
}
private struct OkAntwort: Codable { let ok: Bool? }
private struct EintragAntwort: Codable { let eintrag: ServerEintrag }
private struct FehlerAntwort: Codable {
    let error: String?
    let message: String?
}

enum ServerFehler: Error {
    /// Kein Zugang auf diesem Gerät (noch nicht verbunden).
    case nichtVerbunden
    /// Zugang widerrufen (Profil → Trennen, Abmelden): alles entfernen.
    case widerrufen
    case keinRecht(String)
    case nichtGefunden
    case abgelehnt(String)
    case netz(Error)
    case server(Int, String)

    /// Fehler, wie ihn der Finder versteht.
    var alsFileProviderFehler: Error {
        switch self {
        case .nichtVerbunden, .widerrufen:
            return NSFileProviderError(.notAuthenticated)
        case .keinRecht(let m), .abgelehnt(let m):
            return NSError(domain: NSCocoaErrorDomain, code: NSFileWriteNoPermissionError,
                           userInfo: [NSLocalizedDescriptionKey: m])
        case .nichtGefunden:
            return NSFileProviderError(.noSuchItem)
        case .netz:
            return NSFileProviderError(.serverUnreachable)
        case .server(_, let m):
            return NSError(domain: NSCocoaErrorDomain, code: NSFileReadUnknownError,
                           userInfo: [NSLocalizedDescriptionKey: m, NSLocalizedFailureReasonErrorKey: m])
        }
    }
}

final class Server {
    private let sitzung: URLSession

    init() {
        let k = URLSessionConfiguration.ephemeral
        k.requestCachePolicy = .reloadIgnoringLocalCacheData
        k.urlCache = nil
        k.httpCookieStorage = nil
        k.timeoutIntervalForRequest = 60
        k.timeoutIntervalForResource = 60 * 60
        k.httpAdditionalHeaders = ["User-Agent": "AtlasAblage/1 (macOS)"]
        sitzung = URLSession(configuration: k)
    }

    private func anfrage(_ pfad: String, methode: String = "POST", abfrage: [URLQueryItem] = []) throws -> URLRequest {
        guard let z = ZugangSpeicher.aktuell() else { throw ServerFehler.nichtVerbunden }
        guard var teile = URLComponents(string: z.basis) else { throw ServerFehler.nichtVerbunden }
        teile.path = "/api/public/desktop/ablage/" + pfad
        if !abfrage.isEmpty { teile.queryItems = abfrage }
        guard let url = teile.url else { throw ServerFehler.nichtVerbunden }
        var r = URLRequest(url: url)
        r.httpMethod = methode
        r.setValue("Bearer \(z.token)", forHTTPHeaderField: "Authorization")
        return r
    }

    private func pruefen(_ antwort: URLResponse, _ daten: Data?) throws {
        guard let h = antwort as? HTTPURLResponse else { throw ServerFehler.server(0, "Keine Antwort") }
        if (200..<300).contains(h.statusCode) { return }
        let f = daten.flatMap { try? JSONDecoder().decode(FehlerAntwort.self, from: $0) }
        let meldung = f?.message ?? "Fehler \(h.statusCode)"
        switch h.statusCode {
        case 401:
            if f?.error == "token_ungueltig" { throw ServerFehler.widerrufen }
            throw ServerFehler.nichtVerbunden
        case 403: throw ServerFehler.keinRecht(meldung)
        case 404: throw ServerFehler.nichtGefunden
        case 409, 413, 400: throw ServerFehler.abgelehnt(meldung)
        default: throw ServerFehler.server(h.statusCode, meldung)
        }
    }

    private func json<T: Decodable>(_ r: URLRequest, _ typ: T.Type) async throws -> T {
        let (daten, antwort): (Data, URLResponse)
        do {
            (daten, antwort) = try await sitzung.data(for: r)
        } catch {
            throw ServerFehler.netz(error)
        }
        try pruefen(antwort, daten)
        do {
            return try JSONDecoder().decode(T.self, from: daten)
        } catch {
            throw ServerFehler.server(200, "Unerwartete Antwort")
        }
    }

    private func jsonKoerper(_ r: inout URLRequest, _ wert: [String: Any?]) {
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let sauber = wert.mapValues { $0 ?? NSNull() }
        r.httpBody = try? JSONSerialization.data(withJSONObject: sauber)
    }

    func workspaces() async throws -> [WorkspaceInfo] {
        var r = try anfrage("workspaces")
        jsonKoerper(&r, [:])
        return try await json(r, WorkspacesAntwort.self).workspaces
    }

    /// Alles in einer Anfrage (Server lädt parallel). nil = Server kennt /alles noch nicht.
    func alles() async throws -> AllesAntwort? {
        var r = try anfrage("alles")
        jsonKoerper(&r, [:])
        do {
            return try await json(r, AllesAntwort.self)
        } catch ServerFehler.nichtGefunden {
            return nil
        }
    }

    func liste(_ workspaceId: String) async throws -> ServerListe {
        var r = try anfrage("liste")
        jsonKoerper(&r, ["workspaceId": workspaceId])
        return try await json(r, ServerListe.self)
    }

    /// Lädt die Datei nach `ziel` (überschreibt). Prüft, dass genau so viele
    /// Bytes ankamen, wie der Server angekündigt hat — eine leere oder
    /// abgeschnittene Datei wird nie als Inhalt übergeben.
    func laden(_ dokumentId: String, nach ziel: URL, fortschritt: Progress) async throws -> Int64 {
        let r = try anfrage("datei", methode: "GET", abfrage: [URLQueryItem(name: "id", value: dokumentId)])
        let start = Date()
        let (tmp, antwort): (URL, URLResponse)
        do {
            (tmp, antwort) = try await sitzung.download(for: r)
        } catch {
            protokoll.error("Laden \(dokumentId, privacy: .public): Netz \(error.localizedDescription, privacy: .public)")
            throw ServerFehler.netz(error)
        }
        defer { try? FileManager.default.removeItem(at: tmp) }
        guard let h = antwort as? HTTPURLResponse else { throw ServerFehler.server(0, "Keine Antwort") }
        if !(200..<300).contains(h.statusCode) {
            let daten = try? Data(contentsOf: tmp)
            let text = daten.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            protokoll.error("Laden \(dokumentId, privacy: .public): HTTP \(h.statusCode) \(text, privacy: .public)")
            try pruefen(antwort, daten)
        }
        let groesse = ((try? FileManager.default.attributesOfItem(atPath: tmp.path)[.size]) as? NSNumber)?.int64Value ?? 0
        let erwartet = Int64(h.value(forHTTPHeaderField: "x-atlas-groesse") ?? "")
            ?? Int64(h.value(forHTTPHeaderField: "Content-Length") ?? "")
        protokoll.info("Laden \(dokumentId, privacy: .public): \(groesse) Bytes (erwartet \(erwartet ?? -1)) in \(Date().timeIntervalSince(start)) s")
        if groesse == 0 {
            throw ServerFehler.server(200, "Atlas hat eine leere Datei geliefert. Bitte später erneut öffnen.")
        }
        if let erwartet, erwartet != groesse {
            throw ServerFehler.server(200, "Download unvollständig (\(groesse) von \(erwartet) Bytes).")
        }
        try? FileManager.default.removeItem(at: ziel)
        try FileManager.default.moveItem(at: tmp, to: ziel)
        fortschritt.completedUnitCount = fortschritt.totalUnitCount
        return groesse
    }

    /// Neue Datei hochladen.
    func hochladen(workspaceId: String, ordnerId: String?, name: String, mime: String, datei: URL) async throws -> ServerEintrag {
        var r = try anfrage("hochladen")
        r.setValue(workspaceId, forHTTPHeaderField: "x-atlas-workspace")
        r.setValue(ordnerId ?? "", forHTTPHeaderField: "x-atlas-ordner")
        r.setValue(name.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? "Dokument",
                   forHTTPHeaderField: "x-atlas-name")
        r.setValue(mime, forHTTPHeaderField: "Content-Type")
        return try await hochladenSenden(r, datei)
    }

    func hochladenSenden(_ r: URLRequest, _ datei: URL) async throws -> ServerEintrag {
        let (daten, antwort): (Data, URLResponse)
        do {
            (daten, antwort) = try await sitzung.upload(for: r, fromFile: datei)
        } catch {
            throw ServerFehler.netz(error)
        }
        try pruefen(antwort, daten)
        guard let e = try? JSONDecoder().decode(EintragAntwort.self, from: daten) else {
            throw ServerFehler.server(200, "Unerwartete Antwort")
        }
        return e.eintrag
    }

    /// Bearbeitete Datei als neue Version (Phase 2). `basis` = inhaltVersion,
    /// auf der die Änderung beruht; passt sie nicht mehr, legt Atlas eine
    /// Konfliktkopie an und lässt das Dokument unverändert.
    func neueVersion(dokumentId: String, basis: String, mime: String, datei: URL) async throws -> VersionErgebnis {
        var r = try anfrage("hochladen")
        r.setValue(dokumentId, forHTTPHeaderField: "x-atlas-dokument")
        r.setValue(basis, forHTTPHeaderField: "x-atlas-basis")
        r.setValue(mime, forHTTPHeaderField: "Content-Type")
        let (daten, antwort): (Data, URLResponse)
        do {
            (daten, antwort) = try await sitzung.upload(for: r, fromFile: datei)
        } catch {
            throw ServerFehler.netz(error)
        }
        try pruefen(antwort, daten)
        guard let e = try? JSONDecoder().decode(VersionErgebnis.self, from: daten) else {
            throw ServerFehler.server(200, "Unerwartete Antwort")
        }
        return e
    }

    /// Umbenennen/Verschieben. `ordnerId`: nil = nicht verschieben,
    /// .some(nil) = oberste Ebene des Workspace.
    func aendern(id: String, name: String?, ordnerId: String??) async throws -> ServerEintrag {
        var r = try anfrage("aendern")
        var koerper: [String: Any?] = ["id": id]
        if let name { koerper["name"] = name }
        if let ordnerId {
            if let o = ordnerId { koerper["parentId"] = o } else { koerper["parentId"] = NSNull() }
        }
        jsonKoerper(&r, koerper)
        return try await json(r, EintragAntwort.self).eintrag
    }

    /// Löschen = Atlas-Papierkorb.
    func loeschen(id: String) async throws {
        var r = try anfrage("loeschen")
        jsonKoerper(&r, ["id": id])
        _ = try await json(r, OkAntwort.self)
    }

    func ordnerAnlegen(workspaceId: String, ordnerId: String?, name: String) async throws -> ServerEintrag {
        var r = try anfrage("ordner")
        jsonKoerper(&r, ["workspaceId": workspaceId, "parentId": ordnerId, "name": name])
        return try await json(r, EintragAntwort.self).eintrag
    }
}
