# Araştırma: canlı transkript (2026-11-20)

Kaynak: [yusufipk/dikte](https://github.com/yusufipk/dikte) (Python, PyQt6, GPL-3.0,
216 yıldız) ve Rust tarafındaki konuşma tanıma seçenekleri.

## dikte ne yapıyor

Ctrl+Space ile kayıt başlar, tekrar basınca durur; konuşma yazıya dökülür, temizlenir ve
odaktaki pencereye yapıştırılır. **Gerçek zamanlı akış yok**: kayıt bittikten sonra bütün
ses tek seferde işlenir. Toplantı kaydı, ses/video dosyasından altyazı (`.srt`), geçmiş ve
bir CLI de var.

## Interpres için örnek alınabilecekler

1. **Yerel motor, tek tıkla kurulum.** whisper.cpp ve llama.cpp'yi ayarlar penceresinden
   indirir, SHA-256 ile doğrular (`.part` dosyası tamamlanınca yeniden adlandırılır),
   sunucu olarak açık tutar ve 10 dakika kullanılmazsa belleği bırakır. Makinenin belleğine
   ve ekran kartına uygun modeli işaretler. Interpres'teki OCR model indirmesi de aynı
   ilkeyle yazıldı; ses modeli için aynısı uygulanabilir.
2. **Sessizlik filtresi (VAD).** Whisper sessizlik görünce cümle uydurur ("Altyazı M.K.",
   "Thanks for watching"). dikte, kaydın kendi gürültü tabanının 10 dB üstüne en az 0,3 sn
   çıkmayan sesi atar, -55 dBFS altını atar ve bilinen uydurma cümleleri bir listeyle
   yakalar. Canlı transkriptte bu daha da önemli: her parça API'ye gitmeden süzülmeli.
3. **Yanlış duyulan adları düzeltme.** Sözlükteki adlar konuşma modeline ipucu (prompt),
   temizleme modeline sözlük olarak verilir ("kuber netis" → "Kubernetes"). Interpres'in
   terim sözlüğü buna doğrudan kaynak olabilir.
4. **Temizleme başarısızsa sessiz kalmama.** Ham metin yine verilir, gösterge sarıya döner
   ve nedenini söyler.
5. **İki kanallı toplantı kaydı.** Mikrofon sol, hoparlör çıkışı sağ kanalda kaydedilir;
   kimin konuştuğu kanaldan anlaşılır. Mikrofonun hoparlörden duyduğu yankı, zaman
   örtüşmesi (%50) ve metin benzerliğiyle (%72) atılır.
6. **Dil algılama.** Varsayılan "otomatik"; algılanan dil geçmişe yazılır ve sonraki adımı
   seçer.
7. **Her şeyin bir CLI karşılığı** (`dikte record --seconds 8`, `--json`). Interpres'te
   CLI + MCP ile benzer bir yapı zaten var.
8. **Odak almayan köşe göstergesi** (kayıt noktası, canlı dalga formu, süre).

Lisans notu: dikte GPL-3.0, Interpres AGPL-3.0. Kod kopyalanmıyor; yalnızca fikirler
alınıyor.

## Interpres'te canlı transkript için önerilen yol

**Kullanım:** oyundaki ya da videodaki konuşmayı (sistem sesi) veya mikrofonu dinleyip
anında yazıya dökmek, çevirmek ve oyun katmanındaki altyazı kutusunda göstermek. Ekran
OCR'ının ses karşılığı; mevcut katman, önbellek ve sözlük aynen kullanılır.

**Motor:** [`sherpa-onnx`](https://docs.rs/sherpa-onnx) Rust kütüphanesi (2.0.1).
- Akışlı tanıma (`OnlineRecognizer`, Zipformer): kelime kelime ara sonuç verir; hazır
  modeller çoğunlukla Çince/İngilizce.
- Whisper / SenseVoice + Silero VAD ile "benzetilmiş akış": konuşma parçası bitince
  (cümle sonu) tanınır. Türkçe dahil çok dil için Whisper bu yoldan kullanılır. Gecikme
  cümle başına ~0,5–2 sn.
- Örnekleme hızı dönüştürücü ve mikrofon için `cpal` örnekleri mevcut.

**Deneme sonucu (2026-11-21):** sherpa-onnx 2.0.2 ile `ort` aynı ikilide bağlanıyor ama
çalışırken `ort` açılamıyor ("Failed to initialize ORT API"); `ort` tek başına çalışıyor.
sherpa-onnx bu yüzden elendi. Yerine `whisper-rs` 0.16 (whisper.cpp, ONNX Runtime yok) denendi;
`ort` ile birlikte sorunsuz. espeak-ng ile üretilen yapay sesle, işlemcide (8 iş parçacığı):

| Model | Boyut | İngilizce 4,0 sn | Türkçe 4,5 sn | Türkçe sonuç |
|---|---|---|---|---|
| base | 148 MB | 0,7 sn, kusursuz | 0,8 sn | ilk cümle doğru, ikincisi bozuk |
| large-v3-turbo q5 | 574 MB | 13,5 sn, kusursuz | 14 sn | daha iyi ama yine hatalı |

Dil "otomatik" seçilince Türkçe yanlış algılandı ve süre ikiye katlandı; dil ayardan sabit
verilmeli (otomatik isteğe bağlı). Yapay ses gerçek konuşmadan kötü; Türkçe doğruluk gerçek
kayıtla yeniden ölçülecek. Vulkan (ekran kartı) özelliği derleme betiğinde hata verdi.

**Ses yakalama:**
- Linux: PipeWire'da hoparlör çıkışının "monitor" kaynağı (sistem sesi) ve mikrofon.
- Windows: WASAPI loopback (sistem sesi) ve mikrofon.

**Aşama önerisi (karar kullanıcıda):**
1. Deneme: sherpa-onnx + ort birlikte derleniyor mu, Whisper-base Türkçe/İngilizce gecikmesi.
2. `crates/interpres-speech`: ses yakalama + VAD + tanıma + uydurma cümle filtresi.
3. Katmana bağlama: "Oyun" ekranında kaynak olarak "ekran (OCR)" ya da "ses" seçimi.
4. Mikrofon için dikte benzeri dikte/çeviri kısayolu (isteğe bağlı).

## Ölçüm 2: gerçek Türkçe konuşma, işlemci ve ekran kartı (2026-11-25)

Örnekler: Google FLEURS `tr_tr` test kümesinden 6 cümle (60 sn gerçek konuşma, doğru
metinleriyle). Kelime hata oranı (WER) noktalama ve büyük/küçük harf yok sayılarak; hız ilk
(ısınma) cümle hariç. Dil sabit `tr`. Donanım: 20 çekirdek (8 iş parçacığı), RTX 4070
Laptop 8 GB, CUDA 13.

| Model | Boyut | WER | İşlemci hızı | CUDA hızı |
|---|---|---|---|---|
| base | 148 MB | %37,5 | gerçek zamanın 11,6 katı | 119 katı |
| small | 488 MB | %26,0 | 3,8 katı | 60 katı |
| large-v3-turbo q5 | 574 MB | %8,7 | ~0,3 katı (ilk ölçüm) | 36 katı |

- Türkçe için kullanılabilir doğruluk yalnızca turbo modelde; o da pratikte ekran kartı ister.
- CUDA ikilisi `libcudart`/`libcublas`'a doğrudan bağlı: NVIDIA'sız makinede açılmaz.
- `clang` sistemde yokken `whisper-rs-sys` bağlama üretemiyor;
  `WHISPER_DONT_GENERATE_BINDINGS=1` ile paketteki hazır bağlamalar kullanılıyor.

## Ölçüm 3: Vulkan ve ekran kartı yokken düşme (2026-11-25)

Aynı 6 FLEURS cümlesi, aynı makine.

| Model | WER | Vulkan hızı | CUDA hızı |
|---|---|---|---|
| small | %26,0 | gerçek zamanın 46 katı | 60 katı |
| large-v3-turbo q5 | %8,7 | 32 katı | 36 katı |

- Vulkan ikilisi yalnızca `libvulkan.so.1` (Vulkan yükleyicisi) ile bağlı; CUDA kütüphanesi yok.
- Vulkan sürücüsü gizlenince (`VK_ICD_FILENAMES` boş) whisper.cpp "no GPU found" deyip aynı
  süreçte işlemciye geçiyor; sonuç aynı, `small` gerçek zamanın 3,2 katı. Ayrı yardımcı
  süreç gerekmiyor.
- Kalan durum: Vulkan yükleyicisinin hiç olmadığı makine (ör. ekran kartı sürücüsüz Windows
  sanal makinesi). Çözüm: Windows paketine `vulkan-1.dll` (Khronos yükleyicisi, Apache-2.0)
  eklemek; Linux paketlerinde `libvulkan1` bağımlılık olarak yazılır.
- Vulkan derlemesi bağlama üretmek için libclang ister (paketteki hazır bağlamalarda Vulkan
  işlevleri yok) ve whisper.cpp gölgelendiricileri için `glslc` ister. Yerelde libclang yoksa
  PyPI `libclang` + `BINDGEN_EXTRA_CLANG_ARGS="-I$(gcc -print-file-name=include)"` çalışıyor.

