# Interpres

Genel amaçlı çeviri aracı: büyük belgeleri biçimi bozmadan çevirir, doğrudan yazılan
metni çevirir. Motor: DeepL. (Claude desteği ertelendi.)

*General-purpose translator for documents and text, powered by DeepL.*

> Geliştirme aşamasında. Plan: [docs/PLAN.md](docs/PLAN.md)

## Kurulum

[Releases](https://github.com/efe-clk/interpres/releases) sayfasından indirin:

| Sistem | Dosya |
|---|---|
| Linux | `Interpres_<sürüm>_amd64.AppImage` (çalıştırılabilir yapın) veya `.deb` |
| Windows | `Interpres_<sürüm>_x64-setup.exe` |
| CLI ve MCP | `interpres-cli-<sürüm>-x86_64-linux.tar.gz` / `-windows.zip` |

İlk açılışta Ayarlar'dan DeepL API anahtarını girin. Wayland'de hızlı çeviri için
derleyicide bir kısayolu `interpres-desktop --quick` komutuna bağlayın.

### MCP sunucusu

`interpres-mcp`, Interpres'i MCP istemcilerine açar: metin ve belge çevirisi, sözlük,
geçmiş ve kullanım. DeepL anahtarı uygulamadaki veya `interpres key set deepl` ile
kaydedilen anahtardır.

Claude Code:

```sh
claude mcp add interpres -- /tam/yol/interpres-mcp
```

Claude Desktop ve diğer istemciler (`mcpServers` ayarı):

```json
{ "mcpServers": { "interpres": { "command": "/tam/yol/interpres-mcp" } } }
```

### Daemon

`interpres-daemon` belge kuyruğunu tek bir arka plan sürecinde tutar. Çalışıyorsa
masaüstü uygulaması, `interpres doc` ve MCP sunucusu belgeleri aynı kuyruğa ekler;
CLI'dan eklenen belge uygulamanın Belgeler ekranında görünür. Çalışmıyorsa her biri
belgeyi kendisi çevirir. Linux'ta `$XDG_RUNTIME_DIR/interpres.sock`, Windows'ta
`\\.\pipe\interpres-<kullanıcı>` üzerinden dinler.

Yeni sürüm: `vX.Y.Z` etiketi gönderilince GitHub Actions paketleri derleyip yayımlar.

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
| `crates/interpres-mcp` | `interpres-mcp` MCP sunucusu (stdio) |
| `crates/interpres-daemon` | `interpres-daemon`: paylaşılan belge kuyruğu |
| `apps/desktop` | Tauri v2 masaüstü uygulaması |

## Lisans

[AGPL-3.0](LICENSE)
