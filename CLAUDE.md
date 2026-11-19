# CLAUDE.md — Interpres

Interpres genel amaçlı bir çeviri aracıdır: büyük belgeleri biçimi bozmadan çevirmek,
doğrudan yazılan metni çevirmek ve (v2'de) oyunlarda ekran çevirisi.

Kullanıcı kod bilmiyor. Kodu Claude yazar; derleme, test ve arayüz kontrolünü Claude
çalıştırıp doğrular. Aşama planı: `docs/PLAN.md`.

## İletişim

- Yanıtlar Türkçe.
- Önce bir sonraki adım söylenir. Adımlar numaralı, listeler en fazla 5 madde.
- Giriş cümlesi, tekrar özet, kapanış cümlesi yok.
- Kullanıcının karar vermesi gereken yerde durulur ve tek bir soru sorulur.
- Belirsiz noktada tahmin yürütülmez, sorulur.
- Kesinleşmiş bir kararın değişmesi gerekirse önce gerekçe söylenir, onay beklenir.

## Kesinleşmiş kararlar

- Dil: Rust (workspace). Masaüstü: Tauri v2.
- Arayüz: React + TypeScript + Tailwind + shadcn/ui, ikonlar lucide-react.
- Platformlar (v1): Linux (önce Wayland, X11 de çalışır) ve Windows. Sonra macOS, Android.
- Lisans: AGPL-3.0.
- Async: tokio. Seri hâle getirme: serde. Hatalar: kütüphanelerde `thiserror`,
  uygulama/CLI katmanında `anyhow`. Günlük: `tracing`.

### Motorlar

- Bütün motorlar `interpres-core` içindeki ortak `Engine` trait'ini uygular.
- **DeepL (varsayılan)**
  - Düz metin: `POST /v2/translate`.
  - Belge: `POST /v2/document` (yükle) → `POST /v2/document/{id}` (durum) →
    `POST /v2/document/{id}/result` (indir).
  - Terim sözlükleri: glossary API.
  - Anahtar `:fx` ile bitiyorsa `https://api-free.deepl.com`, değilse `https://api.deepl.com`.
  - Kimlik doğrulama `Authorization: DeepL-Auth-Key <key>` başlığıyla.
- **Claude (isteğe bağlı, ERTELENDİ)**: M5 ve M6 ertelendi (bkz. `docs/PLAN.md`); arayüzde
  Claude seçenekleri gizli, crate boş. Geri alınınca: `claude` Cargo özelliği arkasında. Özellik kapalıyken Claude
  kodu derlemeye hiç girmez (`#[cfg(feature = "claude")]`, crate opsiyonel bağımlılık).
  - a) Claude API: reqwest ile ince Messages API istemcisi (resmi Rust SDK yok).
    SSE akışı, prompt caching, ayardan seçilen model.
  - b) Claude Code: `claude -p --output-format stream-json --verbose` alt süreç.
- Claude hata verirse ya da kurulu değilse çeviri yedek motora (varsayılan DeepL) düşer.

### Belgeler

- DeepL yolu: belge doğrudan DeepL'e gider (PDF dahil).
- Claude yolu: kendi hattımız (`interpres-docs`). docx/pptx/xlsx için `zip` + `quick-xml`;
  metin run bilgisi korunarak çıkarılır, çevrilir, geri yazılır. srt, md, txt de bu yoldan.
- PDF v1'de yalnızca DeepL ile.

### Veri, ayarlar, gizli bilgiler

- SQLite (`rusqlite`, bundled): terim sözlüğü, çeviri belleği, önbellek, geçmiş.
- Önbellek anahtarı: `hash(motor + kaynak dil + hedef dil + resmiyet + sözlük hash'i + metin)`.
  Resmiyet de anahtarda; aynı metnin resmî ve samimi çevirisi farklıdır.
- Sözlükler temel dil çiftine göre tutulur (`TR → EN`) ve yalnızca kaynak dil seçiliyken
  uygulanır. DeepL'de `interpres TR-EN <hash>` adıyla eşitlenir; terimler değişince yeni
  sözlük oluşturulup eskisi silinir.
- Ayarlar: TOML, işletim sisteminin standart ayar klasöründe (`directories` crate).
- API anahtarları: `keyring` ile işletim sisteminin anahtar deposunda. Asla TOML'a,
  günlüğe ya da repoya yazılmaz.

### Oyun (v2)

- Yalnızca ekran yakalama + OCR + katman (overlay). Oyun belleğine hook/inject yok.

## Repo yapısı

```
Cargo.toml                      # workspace
crates/
  interpres-core/               # Engine trait, ortak tipler, Translator servisi,
                                #   sözlük, önbellek, çeviri belleği, geçmiş, ayarlar
  interpres-engine-deepl/       # DeepL metin, belge, glossary
  interpres-engine-claude/      # Claude API + Claude Code (feature: claude)
  interpres-docs/               # belge ayrıştırma / yeniden birleştirme
  interpres-cli/                # `interpres` komut satırı aracı
apps/
  desktop/                      # Tauri v2 (src/ = React, src-tauri/ = Rust)
docs/
  PLAN.md
```

Sonraki sürümler: `interpres-daemon` (IPC), `interpres-mcp` (rmcp), `interpres-overlay`.
Bu yüzden:

- İş mantığı yalnızca `interpres-core` ve motor crate'lerinde durur. CLI, Tauri, daemon ve
  MCP sunucusu aynı `Translator` servisini çağıran ince kabuklardır.
- Core UI'dan, Tauri'den ve süreç modelinden bağımsızdır; tipleri `Serialize/Deserialize`
  eder (IPC ve MCP için hazır).
- Akış, `Stream<Item = Result<Chunk>>` olarak dışarı verilir; tüketici kim olursa olsun aynı.
- İptal için `CancellationToken` (tokio-util) desteklenir.

## Arayüz kuralları (M2 ve sonrası)

- Rust ile iletişim yalnızca `apps/desktop/src/lib/api.ts` üzerinden. Backend'i olmayan
  özellikler için bu dosya sahte veri döndürür; arayüz backend olmadan gezilebilir.
- Bütün arayüz metinleri i18n dosyalarında (`src/i18n/tr.json`, `en.json`). Varsayılan
  Türkçe. Bileşenlerde sabit metin yok.
- Görsel: varsayılan koyu tema, açık tema da var. Minimal, düz, sakin (Omarchy / Linear).
  Az gölge ve gradyan, bol boşluk, tek vurgu rengi.
- Klavye öncelikli: her ana işlemin kısayolu var, Ctrl+K komut paleti.
- Mikro metinler kısa ve nötr; ünlem ve duygusal ifade yok.
- 900x600 pencerede düzgün görünür.
- Her ekranda boş, yükleniyor, hata (anahtar yok / kota doldu / ağ yok) ve başarılı
  durumları tasarlanır.

## Komutlar

```sh
# Rust
cargo build --workspace
cargo build --workspace --all-features          # Claude dahil
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check

# CLI
cargo run -p interpres-cli -- translate --to en "Merhaba dünya"

# Masaüstü (apps/desktop)
npm --prefix apps/desktop ci
npm --prefix apps/desktop run build
npm --prefix apps/desktop run tauri dev
```

### Platform notları

- Linux: `main.rs`, `WEBKIT_DISABLE_DMABUF_RENDERER=1` ayarlar (NVIDIA + Wayland'de
  WebKitGTK "Error 71" ile çöküyor).
- Wayland'de küresel kısayol çalışmaz; hızlı çeviri derleyici kısayolundan
  `interpres-desktop --quick` ile açılır (single-instance eklentisi pencereyi gösterir).
- Hızlı çeviri penceresinin başlığı `Interpres Quick`; pencere kuralları buna göre yazılır.
  `--quick` ile başlatılınca ana pencere gizli kalır, uygulama arka planda bekler.
- Kullanıcının Hyprland'inde Super+Q → `~/.local/bin/interpres-desktop --quick` (o.rebind;
  eskiden pencere kapatmaydı, Super+W hâlâ kapatır) ve `Interpres Quick` için yüzer 560x320 kural.
- Arayüz sahte veriyle tarayıcıda da açılır: `npm --prefix apps/desktop run dev`.

Bir aşama ancak `cargo build`, `cargo test`, `cargo clippy` (ve arayüz aşamalarında
`npm run build`) hatasız geçtiğinde biter.

## Kod kuralları

- Sade ve doğrudan: istenmeyen özellik, tek kullanımlık soyutlama, gereksiz
  yapılandırılabilirlik yok.
- `unwrap()`/`expect()` yalnızca testlerde ve gerçekten imkânsız durumlarda.
- Kütüphane crate'leri kendi `thiserror` hata tipini döndürür; `anyhow` yalnızca CLI ve
  uygulama katmanında.
- Ağ çağrıları `wiremock` ile sahte sunucuya karşı test edilir. Motorlar temel URL'yi
  parametre olarak alır (test edilebilirlik için).
- Gerçek API anahtarı yalnızca kullanıcı verdiğinde kullanılır; anahtarlar hiçbir zaman
  repoya, test verisine ya da günlüğe girmez.
- Kod, yorumlar ve commit mesajları İngilizce; kullanıcıya dönük metinler i18n'de.
- Bağımlılık sürümleri kök `Cargo.toml` içinde `[workspace.dependencies]` altında.

## Git akışı

- Ana dal `main`. Remote: `github.com/efe-clk/interpres` (private).
- Her aşama ayrı bir commit dizisi; aşama bitince push.
- Aşama sonunda `docs/PLAN.md`'de aşama işaretlenir ve kullanıcıya deneme için tek komut
  verilir.
