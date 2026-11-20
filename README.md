# Interpres

Genel amaçlı çeviri aracı: büyük belgeleri biçimi bozmadan çevirir, doğrudan yazılan
metni çevirir. Motor: DeepL. (Claude desteği ertelendi.)

*General-purpose translator for documents and text, powered by DeepL.*

> Geliştirme aşamasında. Plan: [docs/PLAN.md](docs/PLAN.md)

## Kurulum

[Releases](https://github.com/efe-clk/interpres/releases) sayfasından indirin:

| Sistem | Dosya |
|---|---|
| Linux | `Interpres_<sürüm>_amd64.AppImage` (çalıştırılabilir yapın) veya `.deb`; glibc 2.39+ (Ubuntu 24.04+, Fedora 40+, Debian 13, Arch) |
| Windows | `Interpres_<sürüm>_x64-setup.exe` |
| CLI ve MCP | `interpres-cli-<sürüm>-x86_64-linux.tar.gz` / `-windows.zip` |

İlk açılışta Ayarlar'dan DeepL API anahtarını girin.

Uygulama sistem tepsisinde çalışır: pencereyi kapatmak onu tepsiye gönderir, tepsi
menüsünden "Çık" ile kapanır. Küresel kısayollar (Ayarlar'dan değiştirilebilir):

| Kısayol | Komut | İş |
|---|---|---|
| Alt+Q | `interpres-desktop --toggle` | Kapalıysa açar, arkadaysa öne getirir, öndeyse tepsiye gönderir |
| Ctrl+Alt+T | `interpres-desktop --quick` | Panodaki metni hızlı çeviri penceresinde çevirir |

Wayland'de uygulamalar küresel kısayol kaydedemez; derleyicide (ör. Hyprland) bu
komutlara bir kısayol bağlayın.

### Oyun çevirisi

"Oyun" ekranında oyunun yazı dilini seçin, OCR modelini tek tıkla indirin (~20–30 MB),
"Bölge seç" ile altyazının çıktığı alanı çizin ve başlatın. Çeviri, seçilen alanın hemen
altında, tıklamaları oyuna geçiren bir katmanda görünür. Oyunun belleğine dokunulmaz;
yalnızca ekran okunur. Aç/kapat: Ctrl+Alt+O, tepsi menüsü ya da
`interpres-desktop --overlay` (Wayland'de derleyici kısayoluna bağlayın).

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
| `crates/interpres-ocr` | PaddleOCR (ONNX) metin tanıma |
| `crates/interpres-overlay` | Ekran yakalama, oyun katmanı, bölge seçici |
| `apps/desktop` | Tauri v2 masaüstü uygulaması |

## Lisans

[AGPL-3.0](LICENSE)
