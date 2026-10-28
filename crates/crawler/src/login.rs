//! Connexion par formulaire : lecture du formulaire (fonction pure) et cookies de session.

use callistseo_core::FormLogin;
use url::Url;

/// Formulaire prêt à envoyer : adresse d'envoi et champs (cachés compris, identifiants remplis).
#[derive(Debug, PartialEq)]
pub(crate) struct FilledForm {
    pub action: Url,
    pub fields: Vec<(String, String)>,
}

/// Trouve le formulaire de connexion de `html` (celui qui contient un champ mot de passe, ou le
/// champ nommé `password_field`) et le remplit. Les champs cachés (jeton CSRF) sont gardés tels
/// quels ; les boutons et cases non cochées ne sont pas envoyés.
pub(crate) fn fill(html: &str, page: &Url, login: &FormLogin) -> Result<FilledForm, String> {
    use scraper::{Html, Selector};
    let doc = Html::parse_document(html);
    let forms = Selector::parse("form").expect("sélecteur constant");
    let inputs = Selector::parse("input, select, textarea").expect("sélecteur constant");
    let is_password = |e: &scraper::ElementRef| match &login.password_field {
        Some(name) => e.value().attr("name") == Some(name.as_str()),
        None => e
            .value()
            .attr("type")
            .is_some_and(|t| t.eq_ignore_ascii_case("password")),
    };
    let form = doc
        .select(&forms)
        .find(|f| f.select(&inputs).any(|e| is_password(&e)))
        .ok_or("formulaire de connexion introuvable (aucun champ mot de passe)")?;

    let mut fields = Vec::new();
    let (mut user_field, mut pass_field) = (login.username_field.clone(), None);
    for e in form.select(&inputs) {
        let el = e.value();
        let Some(name) = el.attr("name").filter(|n| !n.is_empty()) else {
            continue;
        };
        let kind = el.attr("type").unwrap_or("text").to_ascii_lowercase();
        if is_password(&e) {
            pass_field.get_or_insert_with(|| name.to_owned());
            continue;
        }
        // Champ identifiant par défaut : le dernier champ texte avant le mot de passe.
        if login.username_field.is_none()
            && pass_field.is_none()
            && el.name() == "input"
            && matches!(kind.as_str(), "text" | "email" | "tel")
        {
            user_field = Some(name.to_owned());
        }
        let value = match (el.name(), kind.as_str()) {
            ("input", "submit" | "button" | "image" | "reset" | "file") => continue,
            ("input", "checkbox" | "radio") if el.attr("checked").is_none() => continue,
            ("input", "checkbox" | "radio") => el.attr("value").unwrap_or("on").to_owned(),
            ("textarea", _) => e.text().collect(),
            ("select", _) => {
                let opt = Selector::parse("option").expect("sélecteur constant");
                let mut options = e.select(&opt);
                let chosen = e
                    .select(&opt)
                    .find(|o| o.value().attr("selected").is_some())
                    .or_else(|| options.next());
                match chosen {
                    Some(o) => o
                        .value()
                        .attr("value")
                        .map(str::to_owned)
                        .unwrap_or_else(|| o.text().collect()),
                    None => continue,
                }
            }
            _ => el.attr("value").unwrap_or("").to_owned(),
        };
        fields.push((name.to_owned(), value));
    }
    let pass_field = pass_field.ok_or("champ mot de passe sans nom dans le formulaire")?;
    let user_field = user_field.ok_or(
        "champ identifiant introuvable : précisez son nom (champ « nom du champ identifiant »)",
    )?;
    fields.retain(|(n, _)| *n != user_field);
    fields.push((user_field, login.username.clone()));
    fields.push((pass_field, login.password.clone()));

    // `action` absent ou vide : la page elle-même. Résolu contre `<base href>` comme un lien.
    let base_sel = Selector::parse("base[href]").expect("sélecteur constant");
    let base = doc
        .select(&base_sel)
        .next()
        .and_then(|b| page.join(b.value().attr("href")?).ok())
        .unwrap_or_else(|| page.clone());
    let action = match form.value().attr("action").map(str::trim) {
        Some(a) if !a.is_empty() => base
            .join(a)
            .map_err(|_| "adresse d'envoi du formulaire invalide")?,
        _ => page.clone(),
    };
    if !form
        .value()
        .attr("method")
        .is_none_or(|m| m.eq_ignore_ascii_case("post"))
    {
        return Err(
            "le formulaire de connexion n'est pas envoyé en POST : non pris en charge".into(),
        );
    }
    Ok(FilledForm { action, fields })
}

/// Ajoute à `jar` les cookies des en-têtes `Set-Cookie` (nom=valeur, attributs ignorés ; un cookie
/// vidé ou expiré est retiré).
pub(crate) fn absorb_cookies(
    jar: &mut Vec<(String, String)>,
    headers: &reqwest::header::HeaderMap,
) {
    for v in headers.get_all(reqwest::header::SET_COOKIE) {
        let Ok(v) = v.to_str() else { continue };
        let mut parts = v.split(';');
        let Some((name, value)) = parts.next().and_then(|p| p.split_once('=')) else {
            continue;
        };
        let (name, value) = (name.trim().to_owned(), value.trim().to_owned());
        let expired = value.is_empty()
            || parts.any(|a| {
                let a = a.trim().to_ascii_lowercase();
                a == "max-age=0" || a.starts_with("max-age=-")
            });
        jar.retain(|(n, _)| *n != name);
        if !expired {
            jar.push((name, value));
        }
    }
}

/// En-tête `Cookie` : cookies déjà fournis (`a=b; c=d`) puis ceux de la session, qui l'emportent.
pub(crate) fn cookie_header(given: Option<&str>, jar: &[(String, String)]) -> Option<String> {
    let mut all: Vec<(String, String)> = given
        .unwrap_or("")
        .split(';')
        .filter_map(|p| p.split_once('='))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .filter(|(n, _)| !n.is_empty())
        .collect();
    for (n, v) in jar {
        all.retain(|(m, _)| m != n);
        all.push((n.clone(), v.clone()));
    }
    (!all.is_empty()).then(|| {
        all.iter()
            .map(|(n, v)| format!("{n}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    })
}

/// Liens de déconnexion : jamais suivis pendant un audit connecté (ils fermeraient la session).
pub(crate) fn is_logout(url: &Url) -> bool {
    const WORDS: &[&str] = &[
        "logout",
        "log-out",
        "log_out",
        "signout",
        "sign-out",
        "sign_out",
        "logoff",
        "deconnexion",
        "déconnexion",
        "d%c3%a9connexion",
        "disconnect",
    ];
    let target = format!("{}?{}", url.path(), url.query().unwrap_or("")).to_lowercase();
    WORDS.iter().any(|w| target.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn login() -> FormLogin {
        FormLogin {
            url: "https://a.test/connexion".into(),
            username: "moi@a.test".into(),
            password: "s3cret".into(),
            ..FormLogin::default()
        }
    }

    #[test]
    fn fills_the_password_form_and_keeps_hidden_fields() {
        let html = r#"<form action="/recherche"><input name="q"></form>
            <form method="post" action="/session?x=1">
              <input type="hidden" name="csrf" value="tok">
              <input type="email" name="email" value="">
              <input type="password" name="pwd">
              <input type="checkbox" name="remember" checked>
              <input type="checkbox" name="news">
              <select name="lang"><option value="fr">FR</option><option value="en" selected>EN</option></select>
              <button type="submit" name="go">OK</button><input type="submit" name="s" value="Entrer">
            </form>"#;
        let page = Url::parse("https://a.test/connexion").unwrap();
        let f = fill(html, &page, &login()).unwrap();
        assert_eq!(f.action.as_str(), "https://a.test/session?x=1");
        assert_eq!(
            f.fields,
            [
                ("csrf", "tok"),
                ("remember", "on"),
                ("lang", "en"),
                ("email", "moi@a.test"),
                ("pwd", "s3cret")
            ]
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
        );
    }

    #[test]
    fn explicit_field_names_and_missing_forms() {
        let page = Url::parse("https://a.test/l").unwrap();
        let html = r#"<form method=POST><input name="u" type="text"><input name="code" type="text"><input name="p" type="text"></form>"#;
        let named = FormLogin {
            username_field: Some("u".into()),
            password_field: Some("p".into()),
            ..login()
        };
        let f = fill(html, &page, &named).unwrap();
        assert_eq!(f.action, page, "sans action : la page elle-même");
        assert!(f.fields.contains(&("u".into(), "moi@a.test".into())));
        assert!(f.fields.contains(&("p".into(), "s3cret".into())));
        assert!(fill("<p>rien</p>", &page, &login()).is_err());
        assert!(fill(
            r#"<form method="get"><input name="u"><input type="password" name="p"></form>"#,
            &page,
            &login()
        )
        .is_err());
    }

    #[test]
    fn cookies_are_merged_and_expired_ones_dropped() {
        let mut h = reqwest::header::HeaderMap::new();
        for v in [
            "sid=abc; Path=/; HttpOnly",
            "old=; Max-Age=0",
            "pref=1",
            "pref=2",
        ] {
            h.append(reqwest::header::SET_COOKIE, v.parse().unwrap());
        }
        let mut jar = vec![("old".into(), "x".into())];
        absorb_cookies(&mut jar, &h);
        assert_eq!(
            cookie_header(Some("a=1; sid=ancien"), &jar).as_deref(),
            Some("a=1; sid=abc; pref=2")
        );
        assert_eq!(cookie_header(None, &[]), None);
    }

    #[test]
    fn logout_links_are_recognized() {
        for u in [
            "https://a.test/logout",
            "https://a.test/compte/deconnexion",
            "https://a.test/?action=sign-out",
            "https://a.test/d%C3%A9connexion",
        ] {
            assert!(is_logout(&Url::parse(u).unwrap()), "{u}");
        }
        assert!(!is_logout(
            &Url::parse("https://a.test/blog/connexion-wifi").unwrap()
        ));
    }
}
