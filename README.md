# Interpres

Genel amaçlı çeviri aracı: büyük belgeleri biçimi bozmadan çevirir, doğrudan yazılan
metni çevirir. Motorlar: DeepL (varsayılan) ve isteğe bağlı Claude.

*General-purpose translator for documents and text, powered by DeepL and optionally Claude.*

> Geliştirme aşamasında. Plan: [docs/PLAN.md](docs/PLAN.md)

## Derleme

Rust (stable) gerekir.

```sh
cargo build --workspace
cargo run -p interpres-cli -- --version
```

Claude motoruyla birlikte:

```sh
cargo build --workspace --features interpres-cli/claude
```

## Yapı

| Klasör | İçerik |
|---|---|
| `crates/interpres-core` | Engine trait, ortak tipler, sözlük, önbellek, çeviri belleği |
| `crates/interpres-engine-deepl` | DeepL motoru |
| `crates/interpres-engine-claude` | Claude motoru (`claude` özelliği) |
| `crates/interpres-docs` | Belge ayrıştırma ve yeniden birleştirme |
| `crates/interpres-cli` | `interpres` komut satırı aracı |
| `apps/desktop` | Tauri v2 masaüstü uygulaması |

## Lisans

[AGPL-3.0](LICENSE)
