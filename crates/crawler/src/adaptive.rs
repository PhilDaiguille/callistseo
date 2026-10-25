//! Concurrence adaptative (AIMD) et statistiques en direct d'un audit.
//!
//! Sur un site lent, le débit est ≈ concurrence ÷ latence : c'est la concurrence qui limite, pas le
//! CPU. Le contrôleur la fait monter d'un cran tant que le serveur répond aussi vite qu'au repos, et
//! la divise par deux dès qu'il montre des signes de stress (429, 5xx, erreur réseau) — le schéma
//! « additive increase, multiplicative decrease » de TCP.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Statistiques lisibles pendant l'audit (le serveur les expose par polling).
#[derive(Debug)]
pub struct CrawlStats {
    started: Instant,
    /// URL terminées.
    pub completed: AtomicUsize,
    /// Requêtes en vol.
    pub in_flight: AtomicUsize,
    /// Concurrence autorisée à cet instant (elle varie en mode adaptatif).
    pub limit: AtomicUsize,
    /// URL découvertes en attente d'être visitées.
    pub queued: AtomicUsize,
    /// Limite de pages de l'audit (plafond de la barre de progression).
    pub max_pages: AtomicUsize,
    /// Latence moyenne lissée, en microsecondes.
    pub avg_latency_us: AtomicU64,
    /// Requêtes en échec (erreur réseau, 429, 5xx).
    pub failures: AtomicUsize,
    /// Demande de pause : plus aucune nouvelle requête tant qu'elle est posée (celles en vol finissent).
    pub paused: AtomicBool,
    /// Demande d'arrêt : on n'envoie plus rien et on rend les pages déjà explorées.
    pub stop: AtomicBool,
}

impl Default for CrawlStats {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            completed: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            limit: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            max_pages: AtomicUsize::new(0),
            avg_latency_us: AtomicU64::new(0),
            failures: AtomicUsize::new(0),
            paused: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        }
    }
}

impl CrawlStats {
    pub fn urls_per_second(&self) -> f64 {
        let secs = self.started.elapsed().as_secs_f64();
        if secs <= 0.0 {
            0.0
        } else {
            self.completed.load(Ordering::Relaxed) as f64 / secs
        }
    }

    pub fn avg_latency(&self) -> Duration {
        Duration::from_micros(self.avg_latency_us.load(Ordering::Relaxed))
    }
}

/// Requêtes terminées entre deux décisions.
const WINDOW: u32 = 10;
/// Tant que la latence reste sous `baseline × 1,5`, on peut monter ; au-delà de `× 3`, on descend.
const STABLE: f64 = 1.5;
const INFLATED: f64 = 3.0;
/// Fenêtres sans montée après un incident.
const COOLDOWN: u32 = 3;

pub(crate) struct Controller {
    limit: usize,
    min: usize,
    max: usize,
    window_count: u32,
    window_sum: Duration,
    window_bad: bool,
    /// Meilleure latence moyenne de fenêtre observée (le « repos » du serveur).
    baseline_ms: Option<f64>,
    cooldown: u32,
}

impl Controller {
    /// `start` = concurrence de départ et plancher ; `max` = plafond (= `start` : concurrence fixe).
    pub(crate) fn new(start: usize, max: usize) -> Self {
        let start = start.max(1);
        Self {
            limit: start,
            min: start,
            max: max.max(start),
            window_count: 0,
            window_sum: Duration::ZERO,
            window_bad: false,
            baseline_ms: None,
            cooldown: 0,
        }
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit
    }

    /// Enregistre une requête terminée (`bad` : erreur réseau, 429 ou 5xx) ; renvoie la limite.
    pub(crate) fn record(&mut self, latency: Duration, bad: bool) -> usize {
        self.window_count += 1;
        self.window_sum += latency;
        self.window_bad |= bad;
        if self.window_count < WINDOW {
            return self.limit;
        }
        let avg_ms = self.window_sum.as_secs_f64() * 1000.0 / f64::from(self.window_count);
        if self.window_bad {
            self.limit = (self.limit / 2).max(self.min);
            self.cooldown = COOLDOWN;
        } else {
            // Le « repos » se met à jour vers le bas immédiatement, et dérive lentement vers le haut
            // (une latence qui s'installe durablement plus haut devient la nouvelle normale).
            let base = self.baseline_ms.map_or(avg_ms, |b| (b * 1.02).min(avg_ms));
            self.baseline_ms = Some(base);
            if self.cooldown > 0 {
                self.cooldown -= 1;
            } else if avg_ms <= base * STABLE && self.limit < self.max {
                self.limit += 1;
            } else if avg_ms > base * INFLATED && self.limit > self.min {
                self.limit -= 1;
            }
        }
        (self.window_count, self.window_sum, self.window_bad) = (0, Duration::ZERO, false);
        self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Duration = Duration::from_millis(100);

    /// Fait passer une fenêtre complète de requêtes identiques.
    fn window(c: &mut Controller, latency: Duration, bad: bool) -> usize {
        let mut limit = c.limit();
        for _ in 0..WINDOW {
            limit = c.record(latency, bad);
        }
        limit
    }

    #[test]
    fn climbs_one_step_per_window_while_the_server_is_steady_up_to_the_ceiling() {
        let mut c = Controller::new(4, 8);
        let seen: Vec<_> = (0..6).map(|_| window(&mut c, FAST, false)).collect();
        assert_eq!(seen, [5, 6, 7, 8, 8, 8]);
    }

    #[test]
    fn halves_on_rate_limiting_then_waits_before_climbing_again() {
        let mut c = Controller::new(2, 16);
        for _ in 0..12 {
            window(&mut c, FAST, false);
        }
        assert_eq!(c.limit(), 14);
        assert_eq!(window(&mut c, FAST, true), 7); // 429 / 5xx / erreur réseau : on divise par deux
        assert_eq!(window(&mut c, FAST, false), 7); // pause (3 fenêtres) avant de remonter
        assert_eq!(window(&mut c, FAST, false), 7);
        assert_eq!(window(&mut c, FAST, false), 7);
        assert_eq!(window(&mut c, FAST, false), 8);
    }

    #[test]
    fn never_goes_below_the_start_value() {
        let mut c = Controller::new(4, 16);
        for _ in 0..5 {
            assert!(window(&mut c, FAST, true) >= 4);
        }
        assert_eq!(c.limit(), 4);
    }

    #[test]
    fn backs_off_gently_when_latency_inflates() {
        let mut c = Controller::new(2, 16);
        for _ in 0..6 {
            window(&mut c, FAST, false);
        }
        let peak = c.limit();
        // Le serveur sature : latence ×4 sans erreur -> on redescend d'un cran par fenêtre.
        assert_eq!(window(&mut c, FAST * 4, false), peak - 1);
        assert_eq!(window(&mut c, FAST * 4, false), peak - 2);
    }

    #[test]
    fn fixed_concurrency_never_moves() {
        let mut c = Controller::new(8, 8);
        for bad in [false, false, true, false] {
            assert_eq!(window(&mut c, FAST, bad), 8);
        }
    }

    #[test]
    fn stats_report_rate_and_latency() {
        let s = CrawlStats::default();
        s.completed.store(50, Ordering::Relaxed);
        s.avg_latency_us.store(120_000, Ordering::Relaxed);
        assert!(s.urls_per_second() > 0.0);
        assert_eq!(s.avg_latency(), Duration::from_millis(120));
    }
}
