use std::time::Duration;

/// Règles robots.txt applicables à notre user-agent.
#[derive(Default)]
pub struct Robots {
    /// (autorisé, motif)
    rules: Vec<(bool, String)>,
    delay: Option<Duration>,
    /// Un groupe `User-agent` portant explicitement notre jeton existe (et non seulement `*`).
    named: bool,
}

/// Jeton de produit de notre user-agent : ce qui précède le premier `/` ou espace
/// (`CallistSEO/0.1 (+https://…)` -> `callistseo`). C'est lui que `robots.txt` cible (RFC 9309).
fn product_token(user_agent: &str) -> String {
    user_agent
        .split(|c: char| c == '/' || c.is_whitespace())
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

impl Robots {
    /// Prend le groupe dont un `User-agent` est égal à notre jeton de produit, sinon le groupe `*`.
    // ponytail: pas de fusion de groupes multiples (le groupe nommé remplace `*`).
    pub fn parse(text: &str, agent: &str) -> Self {
        let token = product_token(agent);
        let mut specific: Vec<(bool, String)> = vec![];
        let mut star: Vec<(bool, String)> = vec![];
        let (mut specific_delay, mut star_delay): (Option<Duration>, Option<Duration>) =
            (None, None);
        let (mut in_specific, mut in_star, mut last_was_agent) = (false, false, false);
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, val)) = line.split_once(':') else {
                continue;
            };
            let (key, val) = (key.trim().to_lowercase(), val.trim());
            match key.as_str() {
                "user-agent" => {
                    if !last_was_agent {
                        (in_specific, in_star) = (false, false);
                    }
                    let v = val.to_lowercase();
                    in_star |= v == "*";
                    in_specific |= v != "*" && !v.is_empty() && v == token;
                    last_was_agent = true;
                }
                "allow" | "disallow" => {
                    last_was_agent = false;
                    if val.is_empty() {
                        continue; // "Disallow:" vide = tout autorisé
                    }
                    let rule = (key == "allow", val.to_owned());
                    if in_specific {
                        specific.push(rule);
                    } else if in_star {
                        star.push(rule);
                    }
                }
                "crawl-delay" => {
                    last_was_agent = false;
                    // Secondes, décimales acceptées ; une valeur invalide ou négative est ignorée.
                    let delay = val
                        .parse::<f64>()
                        .ok()
                        .filter(|d| d.is_finite() && *d >= 0.0)
                        .map(Duration::from_secs_f64);
                    if in_specific {
                        specific_delay = delay.or(specific_delay);
                    } else if in_star {
                        star_delay = delay.or(star_delay);
                    }
                }
                _ => last_was_agent = false,
            }
        }
        Self {
            named: !specific.is_empty(),
            rules: if specific.is_empty() { star } else { specific },
            delay: specific_delay.or(star_delay),
        }
    }

    /// Le site interdit tout (`/`) à ce robot **nommé explicitement** (un `*` seul ne compte pas).
    pub fn blocks_named_agent(text: &str, agent: &str) -> bool {
        let r = Self::parse(text, agent);
        r.named && !r.allowed("/")
    }

    /// `Crawl-delay` demandé par le site pour notre agent.
    pub fn crawl_delay(&self) -> Option<Duration> {
        self.delay
    }

    /// Le motif le plus long gagne ; à égalité, `Allow`.
    pub fn allowed(&self, path_and_query: &str) -> bool {
        self.rules
            .iter()
            .filter(|(_, pat)| matches(pat, path_and_query))
            .max_by_key(|(allow, pat)| (pat.len(), *allow))
            .is_none_or(|(allow, _)| *allow)
    }
}

/// Motif robots : préfixe, `*` = n'importe quoi, `$` final = fin d'URL.
fn matches(pat: &str, s: &str) -> bool {
    let (pat, anchored) = pat.strip_suffix('$').map_or((pat, false), |p| (p, true));
    let segs: Vec<&str> = pat.split('*').collect();
    let Some(mut rest) = s.strip_prefix(segs[0]) else {
        return false;
    };
    let Some((last, mid)) = segs[1..].split_last() else {
        return !anchored || rest.is_empty();
    };
    for part in mid {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    if anchored {
        rest.ends_with(last)
    } else {
        rest.contains(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_rules() {
        let r = Robots::parse(
            "User-agent: *\nDisallow: /private\nAllow: /private/ok\nDisallow: /*.pdf$\n\nUser-agent: other\nDisallow: /\n",
            "CallistSEO/0.1",
        );
        assert!(r.allowed("/"));
        assert!(!r.allowed("/private/x"));
        assert!(r.allowed("/private/ok/page")); // Allow plus long
        assert!(!r.allowed("/doc/a.pdf"));
        assert!(r.allowed("/doc/a.pdf?x=1"));
        assert!(r.allowed("/doc/a.pdfx"));
    }

    #[test]
    fn group_is_matched_on_the_product_token_only() {
        let ua = "CallistSEO/0.1.0 (+https://exemple.fr/bot)";
        assert_eq!(product_token(ua), "callistseo");
        let blocked = |txt: &str| !Robots::parse(txt, ua).allowed("/x");
        // Casse ignorée : c'est bien nous.
        assert!(blocked("User-agent: CALLISTSEO\nDisallow: /\n"));
        // Des noms qui ne font que figurer dans la chaîne complète ne nous concernent pas.
        for other in [
            "seo",
            "a",
            "call",
            "bot",
            "exemple",
            "callistseo/0.1.0",
            "https",
        ] {
            assert!(
                !blocked(&format!("User-agent: {other}\nDisallow: /\n")),
                "{other}"
            );
        }
        // Sans groupe nommé, `*` s'applique.
        assert!(blocked("User-agent: *\nDisallow: /\n"));
    }

    #[test]
    fn crawl_delay_named_group_wins_and_bad_values_are_ignored() {
        let ms = |d: Option<Duration>| d.map(|d| d.as_millis());
        let agent = "CallistSEO/0.1";
        let both = "User-agent: *\nCrawl-delay: 5\n\nUser-agent: callistseo\nCrawl-delay: 2\n";
        assert_eq!(ms(Robots::parse(both, agent).crawl_delay()), Some(2000));
        assert_eq!(
            ms(Robots::parse("User-agent: *\nCrawl-delay: 0.5\n", agent).crawl_delay()),
            Some(500)
        );
        assert_eq!(
            ms(Robots::parse("User-agent: other\nCrawl-delay: 9\n", agent).crawl_delay()),
            None
        );
        for bad in ["abc", "-3", "NaN", "inf", ""] {
            let txt = format!("User-agent: *\nCrawl-delay: {bad}\n");
            assert_eq!(Robots::parse(&txt, agent).crawl_delay(), None, "{bad}");
        }
        // Un Crawl-delay seul ne casse pas les règles Allow/Disallow du même groupe.
        let r = Robots::parse("User-agent: *\nCrawl-delay: 1\nDisallow: /x\n", agent);
        assert!(!r.allowed("/x") && r.allowed("/y") && r.crawl_delay().is_some());
    }

    #[test]
    fn specific_group_wins_and_empty_allows() {
        let r = Robots::parse(
            "User-agent: callistseo\nDisallow: /a\nUser-agent: *\nDisallow: /\n",
            "CallistSEO/0.1",
        );
        assert!(!r.allowed("/a") && r.allowed("/b"));
        assert!(Robots::parse("User-agent: *\nDisallow:\n", "x").allowed("/anything"));
        assert!(Robots::default().allowed("/"));
    }

    #[test]
    fn named_agent_blocks_ignore_the_star_group() {
        let txt = "User-agent: *\nDisallow: /\n\nUser-agent: GPTBot\nDisallow: /\n\nUser-agent: ClaudeBot\nAllow: /\n";
        assert!(Robots::blocks_named_agent(txt, "GPTBot"));
        assert!(!Robots::blocks_named_agent(txt, "ClaudeBot")); // groupe nommé qui autorise
        assert!(!Robots::blocks_named_agent(txt, "PerplexityBot")); // seulement `*`
        assert!(!Robots::blocks_named_agent(
            "User-agent: GPTBot\nDisallow: /private\n",
            "GPTBot"
        ));
    }
}
