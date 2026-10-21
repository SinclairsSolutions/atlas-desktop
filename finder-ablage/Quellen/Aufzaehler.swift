// Aufzählungen für den Finder: Ordnerinhalte und die Arbeitsmenge.
//
// Bei replizierten Erweiterungen meldet nur die Arbeitsmenge (working set)
// Änderungen — sie umfasst hier alle Einträge. Ordnerinhalte werden beim
// ersten Öffnen aufgezählt.

import FileProvider
import Foundation

final class Aufzaehler: NSObject, NSFileProviderEnumerator {
    private let container: NSFileProviderItemIdentifier
    private weak var erweiterung: Erweiterung?
    private static let seitenGroesse = 500

    init(container: NSFileProviderItemIdentifier, erweiterung: Erweiterung) {
        self.container = container
        self.erweiterung = erweiterung
    }

    func invalidate() {}

    func enumerateItems(for observer: NSFileProviderEnumerationObserver, startingAt page: NSFileProviderPage) {
        guard let erw = erweiterung else { observer.finishEnumeratingWithError(NSFileProviderError(.notAuthenticated)); return }
        let start = Int(String(data: page.rawValue, encoding: .utf8) ?? "") ?? 0
        Task {
            do {
                // Erste Seite: Nur beim allerersten Mal auf Atlas warten; sonst sofort
                // den bekannten Stand zeigen und im Hintergrund abgleichen.
                if start == 0 {
                    if erw.stand.istLeer { try await erw.abgleichen(maxAlter: 15) } else { erw.abgleichImHintergrund() }
                }
                let liste: [Gespeichert]
                if container == .workingSet {
                    liste = erw.stand.alle()
                } else {
                    let p = container == .rootContainer ? Stand.wurzel : container.rawValue
                    liste = erw.stand.kinder(von: p)
                }
                let ende = min(start + Self.seitenGroesse, liste.count)
                if start < ende { observer.didEnumerate(liste[start..<ende].map(Eintrag.init)) }
                if ende < liste.count {
                    observer.finishEnumerating(upTo: NSFileProviderPage(Data(String(ende).utf8)))
                } else {
                    observer.finishEnumerating(upTo: nil)
                }
            } catch {
                observer.finishEnumeratingWithError(erw.fehler(error))
            }
        }
    }

    func enumerateChanges(for observer: NSFileProviderChangeObserver, from syncAnchor: NSFileProviderSyncAnchor) {
        guard let erw = erweiterung else { observer.finishEnumeratingWithError(NSFileProviderError(.notAuthenticated)); return }
        Task {
            var abgleichFehler: Error?
            do {
                try await erw.abgleichen(maxAlter: 5)
            } catch {
                abgleichFehler = error
            }
            // Auch nach einem Widerruf die Löschungen melden, damit lokale Kopien verschwinden.
            guard let a = erw.stand.aenderungen(seit: syncAnchor.rawValue) else {
                observer.finishEnumeratingWithError(NSFileProviderError(.syncAnchorExpired))
                return
            }
            if container != .workingSet {
                observer.finishEnumeratingChanges(upTo: NSFileProviderSyncAnchor(a.anker), moreComing: false)
                return
            }
            if !a.aktualisiert.isEmpty { observer.didUpdate(a.aktualisiert.map(Eintrag.init)) }
            if !a.geloescht.isEmpty { observer.didDeleteItems(withIdentifiers: a.geloescht.map { NSFileProviderItemIdentifier($0) }) }
            if let f = abgleichFehler, a.aktualisiert.isEmpty, a.geloescht.isEmpty, !(f is CancellationError) {
                observer.finishEnumeratingWithError(erw.fehler(f))
                return
            }
            observer.finishEnumeratingChanges(upTo: NSFileProviderSyncAnchor(a.anker), moreComing: false)
        }
    }

    func currentSyncAnchor(completionHandler: @escaping (NSFileProviderSyncAnchor?) -> Void) {
        completionHandler(erweiterung.map { NSFileProviderSyncAnchor($0.stand.ankerDaten()) })
    }
}

/// Papierkorb: die Ablage hat auf dem Mac keinen eigenen.
final class LeererAufzaehler: NSObject, NSFileProviderEnumerator {
    private let stand: Stand
    init(stand: Stand) { self.stand = stand }
    func invalidate() {}
    func enumerateItems(for observer: NSFileProviderEnumerationObserver, startingAt page: NSFileProviderPage) {
        observer.finishEnumerating(upTo: nil)
    }
    func enumerateChanges(for observer: NSFileProviderChangeObserver, from syncAnchor: NSFileProviderSyncAnchor) {
        observer.finishEnumeratingChanges(upTo: NSFileProviderSyncAnchor(stand.ankerDaten()), moreComing: false)
    }
    func currentSyncAnchor(completionHandler: @escaping (NSFileProviderSyncAnchor?) -> Void) {
        completionHandler(NSFileProviderSyncAnchor(stand.ankerDaten()))
    }
}
