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

## Yol haritası (v0.1 sonrası)

Sürümler yalnızca büyük değişikliklerde ve kullanıcı onayıyla yayımlanır (2026-11-20).
Aşağıdaki sürüm numaraları yalnızca yön gösterir.

Sıra: önce açık kalan işler, sonra çekirdeği dışarı açan servisler (MCP, daemon), sonra
en büyük iş olan oyun katmanı, sonra canlı ses çevirisi. macOS yol haritasından çıkarıldı (kullanıcı kararı, 2026-11-21).
Android (M13) ve Claude motoru (M14) çıkarıldı: yazılım yalnızca bilgisayarda, yalnızca
DeepL ile çalışır (kullanıcı kararı, 2026-11-27). Her aşamanın başında, o aşamadaki açık karar soruları kullanıcıya sorulur.

## [x] M8 — v0.2: Açık işler ve cila

- Hyprland: hızlı çeviri için kısayol (`interpres-desktop --quick`) ve `Interpres Quick`
  penceresini küçük/yüzer açan pencere kuralı. Kullanıcının ayar dosyasına dokunmadan önce
  onay alınır.
- Kısayolların Ayarlar'dan değiştirilebilmesi (ilk planda istenmişti, şu an salt okunur);
  küresel hızlı çeviri kısayolu da dâhil.
- Bir dil çiftinin bütün terimleri silinince DeepL'deki sözlüğün de silinmesi.
- Belgeler ekranında arayüzden dosya ekleme akışının uçtan uca doğrulanması (kullanıcı
  denemesi; gerekirse düzeltme).
- Windows kurulumunun gerçek bir Windows makinesinde denenmesi (makine varsa).

**Bitti:** Hyprland'de kısayol hızlı çeviriyi küçük yüzer pencerede açar; kısayollar
Ayarlar'dan değişir ve kalıcıdır; boşalan çiftin DeepL sözlüğü silinir.

> Not: Super+Q seçildi (o.rebind; pencere kapatma Super+W'de). Fiziksel tuş basışı,
> arayüzden dosya ekleme ve Windows denemesi kullanıcıda.

## [x] M9 — v0.3: MCP sunucusu

- `crates/interpres-mcp`: `rmcp` ile stdio MCP sunucusu; `interpres-mcp` ikilisi.
- Araçlar: `translate_text`, `translate_document` (yol alır, çıktı yolunu döndürür),
  `glossary_list` / `glossary_add` / `glossary_remove`, `history_search`, `usage`.
- Çekirdekteki `Translator`, `DocumentEngine` ve `Store` doğrudan kullanılır; anahtar
  keyring'den okunur.
- Testler: araç çağrıları sahte DeepL (wiremock) ile; MCP el sıkışması ve araç listesi.
- Kurulum notu: Claude Desktop / Claude Code gibi istemcilere nasıl ekleneceği README'de.

**Bitti:** Bir MCP istemcisi Interpres'i ekleyip metin ve belge çevirebilir; ikili sürüme
eklenir.

## [x] M10 — v0.4: Daemon (IPC servisi)

- `crates/interpres-daemon`: arka planda çalışan tek süreç; belge kuyruğu burada yaşar.
  (Uygulamada değişti: sözlük, önbellek ve geçmiş SQLite dosyasında zaten paylaşıldığı
  için daemon'a taşınmadı.)
- IPC: Linux'ta Unix soketi, Windows'ta adlandırılmış boru (named pipe); JSON-RPC benzeri
  istek/yanıt + olay akışı (kuyruk değişiklikleri).
- Masaüstü uygulaması, CLI ve MCP sunucusu daemon varsa ona bağlanır, yoksa bugünkü gibi
  doğrudan çalışır.
- Testler: istemci-sunucu gidiş-dönüş, iki istemcinin aynı kuyruğu görmesi, daemon
  kapanınca doğrudan çalışmaya geçiş.

**Bitti:** CLI'dan eklenen belge masaüstü uygulamasının kuyruğunda görünür; daemon yokken
her şey bugünkü gibi çalışır.

## [x] M10b — Sistem tepsisi ve servis (kullanıcı isteği, 2026-11-20)

- Sistem tepsisi simgesi ve menüsü (göster/gizle, hızlı çeviri, çık); pencere kapatılınca
  tepsiye gider.
- `--toggle` ve küresel "göster/gizle" kısayolu (varsayılan Alt+Q, Ayarlar'dan değişir);
  kullanıcının Hyprland'inde Alt+Q'ya bağlı.
- Daemon için systemd kullanıcı servisi (`packaging/linux/interpres-daemon.service`).

**Bitti:** Alt+Q uygulamayı açar / öne getirir / tepsiye gönderir; tepsi menüsü çalışır;
`systemctl --user status interpres-daemon` etkin ve çökünce yeniden başlar.

## [x] M11 — v2.0: Oyun katmanı (ekran yakalama + OCR + katman)

- `crates/interpres-overlay`. Yalnızca ekran yakalama + OCR + katman; oyun belleğine
  kanca atılmaz (hook/inject yok).
- Yakalama: Linux'ta xdg-desktop-portal ScreenCast (Wayland) ve X11; Windows'ta
  Windows.Graphics.Capture.
- [x] OCR (karar 2026-11-20: PaddleOCR ONNX, tek tıkla kurulum): `crates/interpres-ocr`,
  PP-OCRv5 mobil + gömülü ONNX Runtime; modeller indirilip doğrulanır. Ölçüm: altyazı
  karesi ~50 ms; İngilizce, Japonca ve Türkçe test görüntüleri birebir okundu.
- Katman: seçilen bölgenin üstünde, tıklamaları geçiren şeffaf pencere; çeviri satırları
  yerinde ya da kenarda gösterilir.
- Akış: kısayolla bölge seç → periyodik yakala → metin değiştiyse çevir (önbellek, daemon
  üzerinden) → katmanı güncelle.
- Testler: kayıtlı ekran görüntülerinden OCR + çeviri hattı (ağsız, sahte motorla).

**Bitti:** Hyprland'de ve Windows'ta pencereli bir oyunun altyazısı seçilen bölgeden
okunup çevrilir ve katmanda gösterilir.

> Not (2026-11-20): Hyprland'de uygulama içinden uçtan uca doğrulandı (görüntüleyicide
> gösterilen Japonca altyazı → Türkçe katman). Windows yolu (Tauri pencereleri, xcap)
> CI'da derleniyor ama gerçek bir Windows makinesinde denenmedi.

## [x] M12 — Canlı ses çevirisi (v0.3.0)

Oyundaki/videodaki konuşmayı (sistem sesi) ya da mikrofonu dinleyip konuşma bitince yazıya
dökmek, çevirmek ve altyazı kutusunda göstermek. Ses makineden çıkmaz; yalnızca tanınan metin
çeviri motoruna (DeepL) gider. Ölçümler: [research/canli-transkript.md](research/canli-transkript.md).

Kararlar (öneri, 2026-11-25'te ölçümlerle güncellendi):
- Tanıma: `whisper-rs` (whisper.cpp). sherpa-onnx elendi: kendi ONNX Runtime'ı `ort` ile
  aynı ikilide çakışıyor (denendi). whisper.cpp ile `ort` birlikte çalışıyor (denendi).
  Paketle gelen bağlamalar kullanılır (`WHISPER_DONT_GENERATE_BINDINGS`); libclang gerekmez.
- **Ekran kartı bu aşamanın içinde.** Gerçek Türkçe konuşmada (FLEURS, 6 cümle, 60 sn)
  kelime hata oranı: `base` %37,5, `small` %26, `large-v3-turbo` %8,7. Turbo işlemcide
  kullanılamayacak kadar yavaş, ekran kartında (RTX 4070, CUDA) gerçek zamandan 36 kat hızlı.
- Model seçimi donanıma göre: ekran kartı varsa `large-v3-turbo` (574 MB), yoksa `small`
  (488 MB, işlemcide gerçek zamandan ~4 kat hızlı). `base` sunulmaz. Ayardan değiştirilebilir.
- Ekran kartı yolu: dağıtılan pakette CUDA kütüphanelerine sabit bağ olursa NVIDIA'sız
  makinede uygulama hiç açılmaz. Bu yüzden tanıma, ana uygulamadan ayrı küçük bir yardımcı
  süreçte çalışır (`interpres-speech`); ekran kartlı sürümü açılamazsa işlemci sürümüne düşülür.
  Pakette Vulkan (NVIDIA/AMD/Intel, ek kurulum yok) mı CUDA mı kullanılacağı 1. adımda
  Vulkan da ölçülünce kesinleşir (Vulkan için yerelde `vulkan-headers` paketi gerekiyor).
- Sessizlik algılama (VAD): Silero VAD, mevcut `ort` ile.
- Modeller OCR'daki gibi ilk kullanımda Hugging Face'ten sabit commit'ten indirilir, SHA-256
  ile doğrulanır.
- **Çift dilli altyazı:** üstte orijinal metin (küçük, soluk), altta çeviri. Ayardan
  kapatılabilir; varsayılan açık. Oyun (OCR) altyazısında da aynı ayar kullanılır.

Adımlar:
1. [x] Ekran kartı ölçümü (2026-11-25): Vulkan turbo gerçek zamanın 32 katı (CUDA 36),
   hata oranı aynı; Vulkan sürücüsü yokken aynı süreçte işlemciye düşüyor. Ölçümler
   `docs/research/canli-transkript.md`'de.
2. [x] `crates/interpres-speech` (2026-11-25): ses yakalama (`cpal`; sistem sesi =
   varsayılan çıkış aygıtı giriş olarak: PipeWire sink yakalama / WASAPI loopback), 16 kHz
   mono'ya dönüştürme, Silero VAD + bölütleme (gürültü tabanının 10 dB üstü, 0,25 sn
   altı atılır, 15 sn'de kesilir), Whisper tanıma (yüklerken ısıtılır: soğuk sürücüde ilk
   Vulkan çalışması ~17 sn sürüyor), uydurma cümle süzgeci, sözlük ipucu parametresi.
   Doğrulandı: FLEURS testleri (turbo %0/%17, small %29/%42), canlı sistem sesi denemesi
   (3 cümle, her biri ~0,5 sn'de). Mikrofon canlı denenmedi. Boşta model bırakma ve
   sözlüğün ipucu olarak bağlanması 3. adımda (hattın yaşam döngüsü orada).
3. [x] Hat (2026-11-26): `interpres_speech::listen` yakalama → VAD → Whisper → süzgeç
   zincirini tek iş parçacığında çalıştırır; model dinleme başlayınca yüklenir, durunca
   bırakılır (bekleme sayacı yerine; ısıtma ~0,5 sn). Çift dilli altyazı: çizici orijinali
   üstte küçük ve soluk çizer; `bottom_area` ekranın altında yer verir. CLI:
   `interpres listen --to TR [--from JA] [--mic] [--model small|turbo]`; modeller eksikse
   indirir, kaynak dil verilince sözlük terimleri Whisper'a ipucu olur.
   Doğrulandı: testler; canlı sistem sesinde yakalama → tanıma → DeepL çevirisi çalıştı.
   Canlı denemede kullanıcının o an çalan sesi de yakalandı; sistem sesi denemeleri
   kullanıcı başındayken yapılmaz. Mikrofon denenmedi.
4. [x] Arayüz (2026-11-26): "Oyun" ekranının adı "Canlı" oldu (EN: "Live"; kullanıcı
   isteği). "Ne çevrilsin": ekrandaki yazı (OCR) / konuşma (ses). Ses için kaynak
   (sistem/mikrofon), konuşma dili, model (otomatik/small/turbo), indirme, ekran kartı
   bilgisi; "Orijinali de göster" iki kaynakta da. Başlat/Durdur, durum ve `--overlay`
   bayrağı ortak; ses için ayrı kısayol yok (kullanıcı kararı). Model yüklenirken
   "yükleniyor", hazır olunca "dinleniyor" bildirimi; altyazı penceresi (layer-shell
   olmayan masaüstleri) de çift dilli. Kendiliğinden biten çalıştırma (ör. ses aygıtı yok)
   yeniden başlatmayı engellemez.
   Doğrulandı: `npm run build`, sahte veriyle arayüz (tarayıcıda). Masaüstü uygulamasında
   canlı ses denenmedi (kullanıcının sesini yakalamamak için).
5. [x] Paketleme ve belgeler (2026-11-26): sürüm akışında whisper.cpp ortak işlemci
   tabanıyla derlenir (`packaging/ggml-portable.cmake`: `GGML_NATIVE=OFF`, AVX2/FMA/F16C açık,
   AVX-512 kapalı; yerelde doğrulandı), Windows'ta kısa derleme klasörü ve Vulkan SDK,
   kurulum paketine ve CLI zip'ine `vulkan-1.dll`; `.deb` `libvulkan1` ister. README.

**Bitti:** Hyprland'de bir videonun sesi canlı olarak çevrilip çift dilli altyazıda
görünür; ekran kartıyla turbo model, ekran kartı yokken `small` çalışır; mikrofon da çalışır;
testler, clippy ve arayüz derlemesi geçer; Windows CI derlemesi geçer.

Riskler:
- Vulkan hızı henüz ölçülmedi; CUDA'dan çok yavaşsa pakette hangi yolun kullanılacağı
  kullanıcıya sorulur.
- CUDA paketi büyük (cuBLAS yüzlerce MB); pakete gömülürse indirme boyutu artar.
- Sistem sesini `cpal` ile yakalamak Linux'ta PipeWire monitor kaynağına bağlı; olmazsa
  doğrudan PipeWire kütüphanesi kullanılır.
- Konuşma "cümle bitince" çevrilir; kelime kelime akış yok (gecikme cümle başına ~1–2 sn).

## M13 — Android (çıkarıldı)

Kullanıcı kararıyla yol haritasından çıkarıldı (2026-11-27): mobil sürüm yok.

## M14 — Claude (çıkarıldı)

Kullanıcı kararıyla çıkarıldı (2026-11-27). Boş `interpres-engine-claude` ve
`interpres-docs` crate'leri, `claude` özelliği, Claude motor kimlikleri ve yedek motor
mekanizması silindi.

## Açık işler (2026-11-20, güncelleme 2026-11-21)

Kapatılanlar: çoklu monitörde bölge seçici (her ekranda açılır, tek monitörde denendi),
npm uyarısı (`minimatch` zorlandı), dikey metin OCR'ı, eğik metin OCR'ı (8°–60°,
2026-11-27).

- Windows'ta gerçek deneme: kurulum, oyun katmanı (Tauri pencereleri), bölge seçici.
- GNOME Wayland'de layer-shell yok: seçici tam ekran pencereyle çalışır; çeviri ayrı bir
  pencerede gösterilir ve konumlanamaz (kullanıcıya bildirilir). GNOME'da denenmedi.
- OCR'da 60°'den dik açılı (dikey olmayan) metin desteklenmiyor.
- Paketler imzasız: Windows SmartScreen uyarısı verir.
- Linux paketleri glibc 2.39 istiyor (Ubuntu 22.04 desteklenmiyor).
- Çeviri belleği kaydediliyor ama kullanılmıyor (DeepL'de karşılığı yok).

## Notlar ve riskler

- Linux'ta `keyring` Secret Service ister (gnome-keyring veya KWallet çalışıyor olmalı).
- DeepL metin API'si akış vermez.
- Wayland'de küresel kısayol ve çerçevesiz pencere davranışı derleyiciye göre değişebilir;
  M2'de Hyprland üzerinde doğrulanır.
- Ek crate'ler: `async-trait` (dyn uyumlu async trait), `futures`, `tokio-util`, `sha2`,
  `clap` (CLI), `wiremock` (test).
