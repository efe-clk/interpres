# Interpres

Genel amaçlı çeviri aracı: büyük belgeleri biçimi bozmadan çevirir, doğrudan yazılan
metni çevirir. Motor: DeepL. (Claude desteği ertelendi.)

*General-purpose translator for documents and text, powered by DeepL.*

> Geliştirme aşamasında. Plan: [docs/PLAN.md](docs/PLAN.md)

## Derleme

Rust (stable) gerekir.

```sh
cargo build --workspace
cargo run -p interpres-cli -- --version
```

## Yapı

| Klasör | İçerik |
|---|---|
| `crates/interpres-core` | Engine trait, ortak tipler, sözlük, önbellek, çeviri belleği |
| `crates/interpres-engine-deepl` | DeepL motoru |
| `crates/interpres-engine-claude` | Claude motoru (ertelendi, boş) |
| `crates/interpres-docs` | Belge ayrıştırma ve yeniden birleştirme |
| `crates/interpres-cli` | `interpres` komut satırı aracı |
| `apps/desktop` | Tauri v2 masaüstü uygulaması |

## Lisans

[AGPL-3.0](LICENSE)
