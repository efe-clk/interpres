# Interpres — Plan

Durum işaretleri: `[ ]` yapılmadı, `[x]` bitti.

Her aşamanın ortak "bitti" ölçütü (aşağıdakilere ek olarak):

- `cargo build`, `cargo test`, `cargo clippy -- -D warnings` hatasız.
- Arayüz aşamalarında `npm run build` hatasız.
- CI (Linux + Windows) yeşil.
- Commit + push yapıldı, bu dosyada aşama işaretlendi, kullanıcıya deneme komutu verildi.

---

## [x] M0 — Workspace iskeleti

- Cargo workspace; boş crate'ler: `interpres-core`, `interpres-engine-deepl`,
  `interpres-engine-claude` (feature `claude`), `interpres-docs`, `interpres-cli`.
- `[workspace.dependencies]` ile ortak sürümler; `rust-toolchain.toml` (stable).
- README (Türkçe + kısa İngilizce), LICENSE (AGPL-3.0 tam metin), `.gitignore`.
- GitHub Actions: `ubuntu-latest` ve `windows-latest` üzerinde fmt, build, test, clippy;
  hem varsayılan hem `--all-features`.

**Bitti:** `cargo run -p interpres-cli -- --version` sürüm yazar; CI iki platformda yeşil.

## [x] M1 — Core + DeepL metin çevirisi + CLI

- core: `Lang`, `Formality`, `TranslateRequest`, `Translation`, `Chunk`, `EngineId`,
  `EngineError` (anahtar yok / kota / ağ / geçersiz istek ayrımı).
- `Engine` trait: `translate`, `translate_stream` (akışı olmayan motor tek parça döndürür),
  `languages`, `capabilities` (belge, sözlük, akış, resmiyet desteği).
- `Translator` servisi: motor seçimi + yedek motora düşme iskeleti (M5'te dolar).
- Ayarlar: TOML okuma/yazma (`directories`); anahtarlar `keyring` ile.
- DeepL: `/v2/translate`, `/v2/languages`, `/v2/usage`; Free/Pro uç noktası `:fx` ile.
- CLI: `interpres translate [--from xx] --to yy [--formality ...] "metin"` ve stdin;
  `interpres key set deepl`, `interpres usage`.
- Testler: wiremock ile başarılı yanıt, 403 (anahtar), 456 (kota), ağ hatası,
  Free/Pro URL seçimi.

**Bitti:** `echo "Merhaba" | interpres translate --to en` gerçek anahtarla çeviri döndürür;
hata durumları anlaşılır mesaj verir.

## [x] M2 — Tauri uygulaması + kabuk + tüm ekranlar

- `apps/desktop`: Tauri v2 + Vite + React + TS + Tailwind + shadcn/ui + lucide-react.
- `src/lib/api.ts` tek köprü; hazır olmayan özellikler sahte veri.
- i18n (tr varsayılan, en); koyu/açık tema; Ctrl+K komut paleti; kısayollar.
- Kabuk (kenar çubuğu + motor/bağlantı durumu) ve 7 ekran: Metin, Belgeler, Sözlük,
  Geçmiş, Ayarlar, Hızlı çeviri penceresi (küresel kısayol + pano, Esc ile kapanır).
- Metin ekranı gerçek: DeepL ile çeviri, dil seçiciler, yer değiştirme, resmiyet,
  kopyalama, karakter sayacı, Ctrl+Enter.
- Ayarlar'da DeepL anahtarı gerçek (keyring), Free/Pro otomatik algılama.
- Her ekranda boş / yükleniyor / hata / başarılı durumları.
- CI'ya `npm run build` ve Tauri Linux bağımlılıkları eklenir.

**Bitti:** `npm --prefix apps/desktop run tauri dev` açılır; bütün ekranlar gezilebilir;
Metin ekranı gerçek çeviri yapar; 900x600'de düzgün; Wayland ve X11'de açılır.

## [x] M3 — DeepL belge çevirisi + kuyruk

- DeepL `/v2/document`: yükle → durum sorgula (geri çekilmeli) → indir.
- Kuyruk servisi core'da (UI'dan bağımsız): sıra, ilerleme, iptal, hata.
- Belgeler ekranı gerçek: sürükle-bırak, kuyruk, "klasörde göster", "aç".
- CLI: `interpres doc dosya.docx --to en [--out ...]`.
- Testler: wiremock ile tam döngü, hata ve iptal.

**Bitti:** Bir docx ve bir pdf, arayüzden ve CLI'dan çevrilip biçimi korunmuş olarak iner.

> Not: docx ve pdf gerçek DeepL ile CLI'dan çevrildi, biçim korundu. Arayüzden dosya ekleme
> (Ctrl+O / sürükle-bırak) otomasyonla denenemedi; kullanıcı denemesi bekliyor.

## [x] M4 — Sözlük + önbellek + geçmiş

- SQLite şeması ve göçler (rusqlite, bundled): `glossary`, `tm` (çeviri belleği),
  `cache`, `history`.
- Önbellek anahtarı: motor + dil çifti + metin + sözlük hash'i.
- Sözlük DeepL glossary API ile eşitlenir (dil çifti başına).
- Sözlük ekranı (tablo, ekle/düzenle/sil, CSV içe/dışa), Geçmiş ekranı (arama, Metin'de
  yeniden aç), Ayarlar'da önbelleği temizleme — hepsi gerçek veriye bağlanır.
- CLI: `interpres glossary ...`, `interpres history`.

**Bitti:** Aynı metin ikinci kez ağa gitmeden gelir; sözlük terimi çeviriye uygulanır;
geçmişten kayıt Metin ekranında açılır.

## [x] M7 — Paketleme + sürüm

- Tauri bundler: AppImage, deb (Linux), NSIS (Windows). Uygulama ikonu.
- Etiketle (`v*`) tetiklenen Actions iş akışı → GitHub Releases'a yükleme.
- CLI ikilisi de sürüme eklenir.

**Bitti:** `v0.1.0` etiketi Releases sayfasında AppImage, deb ve setup.exe üretir;
temiz bir makinede kurulup çalışır.

---

## Ertelendi: Claude (M5, M6)

Kullanıcı kararıyla (2026-11-19) Claude motoru ve Claude yolu için belge hattı ertelendi.
Boş `interpres-engine-claude` crate'i ve `claude` özelliği duruyor; arayüzde Claude
seçenekleri gizli. Geri alınırken aşağıdaki plan kullanılır.

### M5 — Claude motoru + yedeğe düşme

- `interpres-engine-claude`, `claude` özelliği arkasında.
- Claude API: reqwest ile Messages API; SSE akışı; prompt caching (sistem talimatı ve
  sözlük önbelleğe alınır); model ayardan. Model listesi ve parametreler bu aşamada güncel
  belgelere göre doğrulanır.
- Claude Code: `claude -p --output-format stream-json --verbose` alt süreç; `claude`
  komutunun varlığı algılanır.
- Yedek motora düşme: Claude hata / yok → DeepL; arayüzde bildirilir.
- Metin ekranında akış hâlinde çıktı; Ayarlar'da Claude API anahtarı, model, Claude Code
  durumu, yedek motor seçimi.
- Testler: wiremock ile SSE akışı ve hatalar; Claude Code için sahte betik ile alt süreç.

**Bitti:** `--features claude` ile Claude API ve Claude Code çeviri yapar; özellik kapalıyken
Claude kodu ikilide yok; Claude kapatılınca çeviri DeepL'den gelir.

### M6 — Claude yolu için belge işleme hattı

- `interpres-docs`: docx, pptx, xlsx (zip + quick-xml), srt, md, txt.
- Run'lar korunarak metin parçaları çıkarılır, işaretli segmentler hâlinde çevrilir,
  aynı yapıya geri yazılır. Büyük belgede parçalama + sıra korunur, eksik segment kontrolü.
- Kuyruk, motor Claude seçildiğinde bu hattı kullanır.
- Testler: örnek dosyalarla gidiş-dönüş (çeviri yerine kimlik fonksiyonu) bayt/yapı
  eşitliği; kalın/italik/bağlantı korunur.

**Bitti:** Örnek docx/pptx/xlsx/srt/md Claude ile çevrilir, Word/LibreOffice'te biçimi
bozulmadan açılır, hiçbir segment atlanmaz.

## Sonrası (v2+)

- `interpres-mcp`: rmcp ile MCP sunucusu (Translator servisini sunar).
- `interpres-daemon`: IPC servisi; masaüstü, CLI ve MCP aynı süreci paylaşabilir.
- `interpres-overlay`: ekran yakalama + OCR + katman (hook/inject yok).
- macOS, Android.

## Notlar ve riskler

- Linux'ta `keyring` Secret Service ister (gnome-keyring veya KWallet çalışıyor olmalı).
- DeepL metin API'si akış vermez; akış yalnızca Claude motorlarında görünür.
- Wayland'de küresel kısayol ve çerçevesiz pencere davranışı derleyiciye göre değişebilir;
  M2'de Hyprland üzerinde doğrulanır.
- Ek crate'ler: `async-trait` (dyn uyumlu async trait), `futures`, `tokio-util`, `sha2`,
  `clap` (CLI), `wiremock` (test).
