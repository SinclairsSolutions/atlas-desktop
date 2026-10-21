// Gerätezugang der Erweiterung („Ablage im Finder“).
//
// Die App holt den Token über die angemeldete Atlas-Seite und legt ihn einmal
// als Übergabe-Datei in den gemeinsamen App-Gruppen-Ordner. Die Erweiterung
// übernimmt ihn beim nächsten Aufruf in ihren eigenen Schlüsselbund und
// löscht die Datei. Der Token gilt nur für /api/public/desktop/ablage/* und
// ist im Profil (Apps) jederzeit widerrufbar.

import Foundation
import Security

enum Konstanten {
    /// App-Gruppe (macOS-Stil mit Team-ID, siehe README „Ablage im Finder“).
    static let appGruppe = "KJ63M9QZUG.com.sinclairs-solutions.atlas"
    static let schluesselbundDienst = "com.sinclairs-solutions.atlas.ablage"
    static let schluesselbundKonto = "geraet"
}

struct Zugang: Codable, Equatable {
    let token: String
    /// z. B. https://sinclair-atlas.com
    let basis: String
    let geraetId: String
}

enum ZugangSpeicher {
    private static let sperre = NSLock()
    private static var zwischenspeicher: Zugang?

    static func uebergabeURL() -> URL? {
        FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: Konstanten.appGruppe)?
            .appendingPathComponent("Ablage", isDirectory: true)
            .appendingPathComponent("zugang.json")
    }

    /// Aktueller Zugang oder nil (dann ist die Ablage nicht verbunden).
    static func aktuell() -> Zugang? {
        sperre.lock()
        defer { sperre.unlock() }
        if let url = uebergabeURL(), FileManager.default.fileExists(atPath: url.path) {
            if let daten = try? Data(contentsOf: url),
               let z = try? JSONDecoder().decode(Zugang.self, from: daten),
               z.basis.hasPrefix("https://") || z.basis.hasPrefix("http://localhost") || z.basis.hasPrefix("http://127.0.0.1")
            {
                if schluesselbundSchreiben(daten) {
                    try? FileManager.default.removeItem(at: url)
                }
                zwischenspeicher = z
                return z
            }
        }
        if let z = zwischenspeicher { return z }
        if let daten = schluesselbundLesen(), let z = try? JSONDecoder().decode(Zugang.self, from: daten) {
            zwischenspeicher = z
            return z
        }
        return nil
    }

    /// Nach Widerruf: Token vergessen.
    static func vergessen() {
        sperre.lock()
        defer { sperre.unlock() }
        zwischenspeicher = nil
        SecItemDelete(basisAbfrage() as CFDictionary)
    }

    private static func basisAbfrage() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Konstanten.schluesselbundDienst,
            kSecAttrAccount as String: Konstanten.schluesselbundKonto,
        ]
    }

    private static func schluesselbundLesen() -> Data? {
        var q = basisAbfrage()
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var ergebnis: AnyObject?
        guard SecItemCopyMatching(q as CFDictionary, &ergebnis) == errSecSuccess else { return nil }
        return ergebnis as? Data
    }

    private static func schluesselbundSchreiben(_ daten: Data) -> Bool {
        SecItemDelete(basisAbfrage() as CFDictionary)
        var q = basisAbfrage()
        q[kSecValueData as String] = daten
        q[kSecAttrLabel as String] = "Atlas – Ablage im Finder"
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(q as CFDictionary, nil) == errSecSuccess
    }
}
