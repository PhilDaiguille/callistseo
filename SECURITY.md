# Politique de sécurité

CallistSEO est en construction, avant bêta : seule la dernière version de `main` est suivie.

## Signaler une vulnérabilité

**Ne publiez pas de détails dans une issue publique.** Utilisez le signalement privé de GitHub :
onglet *Security* du dépôt → *Report a vulnerability*
(<https://github.com/PhilDaiguille/callistseo/security/advisories/new>).

Indiquez la version ou le commit, les étapes de reproduction et l'impact estimé. On vise un accusé de réception sous 7 jours.

## Ce qui est dans le périmètre

- **SSRF** : contournement du filtre d'IP privées (DNS rebinding, redirections, IPv6, NAT64), voir `crates/crawler/src/client.rs`.
- **Authentification** de l'API (jeton Bearer), fuite du jeton, redirection ouverte sur `/login?next=`.
- **Injection** : SQL (liste blanche de `storage/src/reports.rs`), formules CSV, HTML des rapports exportés.
- **Épuisement de ressources** : limites de pages, de taille de réponse, regex du périmètre.

## Hors périmètre

- `--allow-private` et la CLI en local : voulus pour les tests et l'usage sur son propre poste.
- Absence de HTTPS intégré, de comptes nominatifs, de limitation de débit : connus, listés dans [`ROADMAP.md`](ROADMAP.md). Exposez la version web derrière un proxy HTTPS.
- Dépendances indirectes déjà signalées par `cargo audit` (voir « Dette connue » dans la roadmap).

## Usage responsable

N'auditez que des sites dont vous êtes propriétaire ou pour lesquels vous avez une autorisation.
