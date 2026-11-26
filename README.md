# Interpres

Genel amaçlı çeviri aracı: büyük belgeleri biçimi bozmadan çevirir, doğrudan yazılan
metni çevirir, ekrandaki yazıyı ve konuşmayı canlı çevirir. Motor: DeepL.

*General-purpose translator for documents, text, on-screen text and live speech, powered by DeepL.*

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

### Canlı çeviri: ekrandaki yazı

"Canlı" ekranında "Ne çevrilsin" için "Ekrandaki yazı (OCR)" seçin, yazının dilini seçin, OCR modelini tek tıkla indirin (~20–30 MB),
"Bölge seç" ile altyazının çıktığı alanı çizin ve başlatın. Çeviri, seçilen alanın hemen
altında, tıklamaları oyuna geçiren bir katmanda görünür. Oyunun belleğine dokunulmaz;
yalnızca ekran okunur. Aç/kapat: Ctrl+Alt+O, tepsi menüsü ya da
`interpres-desktop --overlay` (Wayland'de derleyici kısayoluna bağlayın).

### Canlı çeviri: konuşma

"Canlı" ekranında "Konuşma (ses)" seçin: sistem sesini (oyun, video, görüşme) ya da
mikrofonu dinler, her cümle bitince çevirisini ekranın altında gösterir; isterseniz
orijinali de üstünde. Konuşma bilgisayarda yazıya dökülür (Whisper); ses dışarı gitmez,
yalnızca yazıya dökülen metin DeepL'e gider.

- Ekran kartı varsa (NVIDIA, AMD, Intel; Vulkan) daha doğru `turbo` modeli kullanılır
  (~575 MB), yoksa işlemcide çalışan `small` (~490 MB). İlk kullanımda bir kez indirilir.
- Konuşma dilini seçmek doğruluğu artırır; seçilince o dil çiftinin sözlük terimleri de
  doğru yazım için ipucu olarak kullanılır.
- Komut satırından: `interpres listen --to TR --from EN` (`--mic` mikrofon için).
- İşlemci tabanı: AVX2 (2013 sonrası x86-64).

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

Rust (stable), CMake, libclang, Vulkan başlıkları ve `glslc` gerekir (Arch: `clang`,
`vulkan-headers`, `shaderc`; Ubuntu: `clang`, `libvulkan-dev`, `glslc`, `libasound2-dev`).

```sh
cargo build --workspace
cargo run -p interpres-cli -- --version
```

## Yapı

| Klasör | İçerik |
|---|---|
| `crates/interpres-core` | Engine trait, ortak tipler, sözlük, önbellek, çeviri belleği |
| `crates/interpres-engine-deepl` | DeepL motoru |
| `crates/interpres-cli` | `interpres` komut satırı aracı |
| `crates/interpres-mcp` | `interpres-mcp` MCP sunucusu (stdio) |
| `crates/interpres-daemon` | `interpres-daemon`: paylaşılan belge kuyruğu |
| `crates/interpres-ocr` | PaddleOCR (ONNX) metin tanıma |
| `crates/interpres-speech` | Ses yakalama, konuşma algılama, Whisper ile yazıya dökme |
| `crates/interpres-overlay` | Ekran yakalama, altyazı katmanı, bölge seçici |
| `apps/desktop` | Tauri v2 masaüstü uygulaması |

## Lisans

[AGPL-3.0](LICENSE)
