// Pas de console sur Windows en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use callistseo_api::{ApiConfig, InProcessResponse, LocalApi, SecretKey};
use callistseo_storage::Store;
use std::path::Path;
use tauri::Manager;

/// L'interface appelle l'API Rust en processus : aucun port n'est ouvert.
#[tauri::command]
async fn api_request(
    api: tauri::State<'_, LocalApi>,
    method: String,
    path: String,
    body: Option<String>,
) -> Result<InProcessResponse, String> {
    api.call(&method, &path, body).await
}

/// Enregistre un export via la boîte de dialogue native ; `false` si l'utilisateur annule.
#[tauri::command]
async fn save_file(
    app: tauri::AppHandle,
    filename: String,
    content: String,
) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    // Seul le nom de fichier proposé vient de l'interface ; l'emplacement est choisi par l'utilisateur.
    let name = Path::new(&filename)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("nom de fichier invalide")?
        .to_owned();
    let Some(target) = app.dialog().file().set_file_name(name).blocking_save_file() else {
        return Ok(false);
    };
    let target = target.into_path().map_err(|e| e.to_string())?;
    std::fs::write(target, content).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Ouvre une URL web dans le navigateur du système. Seuls http et https sont acceptés : l'interface
/// ne peut pas lancer un autre programme ou un fichier local par ce biais.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("seules les URL http et https peuvent être ouvertes".into());
    }
    let target = parsed.as_str();
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Clé des identifiants des sites audités : `CALLISTSEO_SECRET_KEY` si défini, sinon un fichier
/// `secret.key` créé au premier lancement dans le dossier de l'application (lisible par
/// l'utilisateur seul). Elle protège les identifiants si la base seule est copiée ou partagée.
// ponytail: fichier à côté de la base ; trousseau du système (Keychain, Credential Manager) si besoin.
fn secret_key(dir: &std::path::Path) -> Result<SecretKey, String> {
    if let Ok(k) = std::env::var("CALLISTSEO_SECRET_KEY") {
        return SecretKey::from_hex(&k);
    }
    let path = dir.join("secret.key");
    if !path.exists() {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        use std::io::Write;
        opts.open(&path)
            .and_then(|mut f| f.write_all(SecretKey::generate_hex().as_bytes()))
            .map_err(|e| format!("clé {} : {e}", path.display()))?;
    }
    let hex =
        std::fs::read_to_string(&path).map_err(|e| format!("clé {} : {e}", path.display()))?;
    SecretKey::from_hex(&hex)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let mut store = Store::open(&dir.join("audits.db"))?;
            // Un audit resté « en cours » à la fermeture précédente est marqué interrompu.
            store.fail_stale_runs()?;
            let cfg = ApiConfig {
                secret_key: Some(secret_key(&dir)?),
                ..ApiConfig::default()
            };
            let api = LocalApi::new(store, cfg);
            // Audits récurrents : ils ne tournent que pendant que l'application est ouverte.
            tauri::async_runtime::spawn(api.scheduler().run());
            app.manage(api);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api_request, save_file, open_url])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de CallistSEO");
}
