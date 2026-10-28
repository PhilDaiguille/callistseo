//! Identifiants des sites audités, chiffrés en AES-256-GCM avant d'aller en base.
//!
//! Format : `1 ‖ nonce (12 octets, aléatoire) ‖ texte chiffré + étiquette`. La donnée associée est
//! l'identifiant du projet : un blob recopié sur un autre projet ne se déchiffre pas.

use aes_gcm::aead::{Aead, Generate, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use callistseo_core::SiteAuth;
use serde::{Deserialize, Serialize};

const VERSION: u8 = 1;
const NONCE_LEN: usize = 12;

/// Clé de 32 octets (`CALLISTSEO_SECRET_KEY`, 64 caractères hexadécimaux). Le `Debug` la masque.
#[derive(Clone)]
pub struct SecretKey([u8; 32]);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(***)")
    }
}

impl SecretKey {
    /// 64 caractères hexadécimaux (`openssl rand -hex 32`).
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let hex = hex.trim();
        let err = || {
            "CALLISTSEO_SECRET_KEY : 64 caractères hexadécimaux attendus (openssl rand -hex 32)"
                .to_owned()
        };
        if hex.len() != 64 || !hex.is_ascii() {
            return Err(err());
        }
        let mut key = [0u8; 32];
        for (i, b) in key.iter_mut().enumerate() {
            *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| err())?;
        }
        Ok(Self(key))
    }

    /// Nouvelle clé aléatoire, en hexadécimal (fichier de clé de l'application desktop).
    pub fn generate_hex() -> String {
        let key = aes_gcm::Key::<Aes256Gcm>::generate();
        key.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(&self.0.into())
    }
}

/// Donnée associée : nature du secret et projet. Un blob d'une table ou d'un projet ne se
/// déchiffre pas ailleurs.
fn aad(label: &str, project_id: i64) -> Vec<u8> {
    match label {
        // Valeur d'origine des identifiants de site (blobs déjà enregistrés).
        "site" => format!("callistseo:projet:{project_id}").into_bytes(),
        _ => format!("callistseo:{label}:{project_id}").into_bytes(),
    }
}

/// Forme JSON des identifiants (corps de `PUT /api/projects/{id}/auth`, et texte chiffré).
#[derive(Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct AuthBody {
    pub basic: Option<BasicBody>,
    pub headers: Vec<HeaderBody>,
    pub cookies: Option<String>,
    pub form: Option<FormBody>,
}

/// Connexion par formulaire. Les noms de champs sont facultatifs (détection automatique).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormBody {
    pub url: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub username_field: Option<String>,
    #[serde(default)]
    pub password_field: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasicBody {
    pub user: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderBody {
    pub name: String,
    pub value: String,
}

impl AuthBody {
    pub fn to_core(&self) -> SiteAuth {
        SiteAuth {
            basic: self
                .basic
                .as_ref()
                .map(|b| (b.user.clone(), b.password.clone())),
            headers: self
                .headers
                .iter()
                .map(|h| (h.name.clone(), h.value.clone()))
                .collect(),
            cookies: self.cookies.clone().filter(|c| !c.is_empty()),
            form: self.form.as_ref().map(|f| callistseo_core::FormLogin {
                url: f.url.trim().to_owned(),
                username: f.username.clone(),
                password: f.password.clone(),
                username_field: f.username_field.clone().filter(|n| !n.is_empty()),
                password_field: f.password_field.clone().filter(|n| !n.is_empty()),
            }),
        }
    }
}

pub fn encrypt(key: &SecretKey, project_id: i64, auth: &AuthBody) -> Result<Vec<u8>, String> {
    seal(key, "site", project_id, auth)
}

pub fn decrypt(key: &SecretKey, project_id: i64, blob: &[u8]) -> Result<AuthBody, String> {
    open(key, "site", project_id, blob)
}

/// Chiffre une valeur JSON sous l'étiquette `label` (`site`, `integrations`).
pub fn seal<T: Serialize>(
    key: &SecretKey,
    label: &str,
    project_id: i64,
    value: &T,
) -> Result<Vec<u8>, String> {
    let plain = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let nonce = Nonce::generate();
    let sealed = key
        .cipher()
        .encrypt(
            &nonce,
            Payload {
                msg: &plain,
                aad: &aad(label, project_id),
            },
        )
        .map_err(|_| "chiffrement impossible".to_owned())?;
    let mut out = Vec::with_capacity(1 + NONCE_LEN + sealed.len());
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Échoue si la clé a changé, si le blob est abîmé ou s'il vient d'un autre projet ou d'une autre table.
pub fn open<T: serde::de::DeserializeOwned>(
    key: &SecretKey,
    label: &str,
    project_id: i64,
    blob: &[u8],
) -> Result<T, String> {
    let unreadable = || {
        "identifiants illisibles (CALLISTSEO_SECRET_KEY a changé ?) : enregistrez-les à nouveau"
            .to_owned()
    };
    if blob.len() < 1 + NONCE_LEN || blob[0] != VERSION {
        return Err(unreadable());
    }
    let nonce = Nonce::try_from(&blob[1..1 + NONCE_LEN]).map_err(|_| unreadable())?;
    let plain = key
        .cipher()
        .decrypt(
            &nonce,
            Payload {
                msg: &blob[1 + NONCE_LEN..],
                aad: &aad(label, project_id),
            },
        )
        .map_err(|_| unreadable())?;
    serde_json::from_slice(&plain).map_err(|_| unreadable())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> SecretKey {
        SecretKey::from_hex(&c.to_string().repeat(64)).unwrap()
    }

    #[test]
    fn round_trip_is_bound_to_the_key_and_the_project() {
        let auth = AuthBody {
            basic: Some(BasicBody {
                user: "u".into(),
                password: "motdepasse".into(),
            }),
            ..AuthBody::default()
        };
        let blob = encrypt(&key('a'), 7, &auth).unwrap();
        assert!(!String::from_utf8_lossy(&blob).contains("motdepasse"));
        let back = decrypt(&key('a'), 7, &blob).unwrap();
        assert_eq!(back.basic.unwrap().password, "motdepasse");
        // Autre clé, autre projet, blob modifié : refusés.
        assert!(decrypt(&key('b'), 7, &blob).is_err());
        assert!(decrypt(&key('a'), 8, &blob).is_err());
        let mut bad = blob.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(decrypt(&key('a'), 7, &bad).is_err());
        // Un blob d'identifiants ne passe pas pour des clés d'intégration.
        assert!(open::<serde_json::Value>(&key('a'), "integrations", 7, &blob).is_err());
        // Deux chiffrements du même contenu diffèrent (nonce aléatoire).
        assert_ne!(encrypt(&key('a'), 7, &auth).unwrap(), blob);
    }

    #[test]
    fn keys_are_parsed_strictly_and_never_printed() {
        assert!(SecretKey::from_hex("abc").is_err());
        assert!(SecretKey::from_hex(&"g".repeat(64)).is_err());
        assert!(SecretKey::from_hex(&"é".repeat(32)).is_err());
        let hex = SecretKey::generate_hex();
        assert_eq!(hex.len(), 64);
        let k = SecretKey::from_hex(&hex).unwrap();
        assert!(!format!("{k:?}").contains(&hex[..8]));
    }
}
