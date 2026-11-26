![PASTI](https://raw.githubusercontent.com/rifqi2320/pasti/main/assets/banner.png)

Linter buat **PASTI** (Panduan Standardisasi Teks dan Istilah), spec bahasa Indonesia teknis terkendali. Repo ini implement spec `0.1.0-draft.1`.

Linter ini cuma ngecek rule yang bisa dicek secara mekanis, dan report-nya selalu nyebutin rule mana yang nggak dicek. File kamu nggak pernah diubah.

> PASTI ini inisiatif independen. Nggak ada hubungannya sama aplikasi *Pasti* (Padanan Istilah) dari Badan Bahasa, dan nggak di-endorse ASD/STEMG. Threshold dan word list di sini masih draft policy. Kami belum ngukur precision, recall, atau dampaknya ke pembaca.

## Install

```sh
uv tool install pasti        # atau sekali jalan: uvx pasti lint manual.md
pip install pasti            # wheel yang sama, binary-nya udah prebuilt, nggak perlu Rust
cargo install --git https://github.com/rifqi2320/pasti
```

## Cara pakai

```sh
pasti lint docs/manual.md --config pasti.yaml     # output buat dibaca manusia
pasti lint docs/*.md --format json --all          # full report, termasuk yang lolos
pasti lint manual.md --gate --require-complete    # buat gate di CI
pasti compare sumber.txt revisi.txt --gate        # cek apakah revisi ngubah makna
pasti parse --text "Matikan pompa_utama." --registry examples/registry-demo.json
pasti count manual.md                             # lihat cara kata dihitung
pasti rules                                       # daftar semua rule di spec
```

`pasti lint` ngecek Markdown (CommonMark + tabel + math) dan plain text. Tiap diagnostic nunjuk posisi persis di source, dan report-nya punya bagian coverage yang misahin rule yang udah dicek dari yang belum.

`pasti compare` ngebandingin teks sumber sama revisinya, terus nandain bagian yang maknanya mungkin berubah. Misalnya `boleh` diganti `harus`, `sudah` atau `Jangan` hilang, `mL` jadi `L`, `lebih dari` jadi `atau lebih`, atau syarat yang ketinggalan waktu satu kalimat dipecah jadi dua.

`pasti parse` ngurai satu kalimat di profil formal `PASTI-FOR-EX01` jadi AST yang typed, pakai registry entitas. Hasilnya cuma representasi dan nggak dieksekusi.

`pasti count` nampilin tiap kalimat beserta unit yang dihitung, biar penulis bisa cross-check hitungan panjang kalimat. `pasti rules` nampilin semua rule ID di spec dan nandain mana yang dicek otomatis.

Report yang bersih belum berarti dokumen kamu sesuai spec (PASTI-CORE-007). Status paling tinggi yang bisa dikasih tool ini cuma `diperiksa_otomatis`.

### Exit code

| Kode | Artinya |
| --- | --- |
| 0 | Pengecekan selesai, gate lolos atau nggak dipakai. Tanpa `--gate`, temuan apa pun nggak ngubah exit code. |
| 1 | Pengecekan selesai, tapi gate gagal. |
| 2 | Ada input, config, atau registry yang gagal dibaca. Report-nya ditandai `partial: true`. |

Output text pakai format `path:baris:kolom` (kolom mulai dari 1). Output JSON ikut koordinat spec: Unicode code point, `line` mulai dari 1, `column_cp` dan `offset_cp` mulai dari 0, range-nya half-open.

## Jenis teks (mode)

Beberapa rule tergantung jenis teksnya: `PROC`, `DESC`, `REQ`, `WARN`, atau `UI`. Kamu set default-nya lewat `default_mode` atau `--mode`. Di dalam Markdown, ganti mode pakai directive:

```markdown
<!-- pasti: mode=WARN -->
Permukaan panas dapat menyebabkan luka bakar.

<!-- pasti: mode=PROC -->
1. Matikan pompa utama.

<!-- pasti: exact -->
> Klik Restart.
```

`mode=...` berlaku sampai ketemu directive mode berikutnya. `exact` nandain blok sesudahnya sebagai kutipan sumber, jadi rule prosa nggak nyentuh blok itu.

Kalau mode belum diset, rule yang butuh mode keluar sebagai `not_evaluated` (`MODE_UNKNOWN`). Tool ini nggak nebak threshold.

## Rule yang dicek otomatis

| Rule | Yang dicek | Hasil |
| --- | --- | --- |
| PASTI-TOK-005 | Kalimat lewat threshold review (PROC/WARN > 20, DESC/REQ > 25 unit; UI cuma kalau di-config) | `possible_violation`, review pending. Kalimat panjang belum tentu melanggar. |
| PASTI-TOK-002 | Singkatan ambigu (`dll.`) nggak dipecah jadi kalimat baru, ketidakpastiannya dilaporin | `possible_violation`, low |
| PASTI-TOK-001 | Blok HTML mentah nggak diurai sebagai prosa | `not_evaluated` |
| PASTI-NUM-001 | Titik desimal (`2.5`), nol di depan yang hilang (`,5`), pemisah ambigu (`1.000`) | violation atau kandidat |
| PASTI-NUM-002 | Angka nempel ke satuan (`80°C`, `5kg`) atau kapitalisasi satuan salah (`5 KG`) | violation atau kandidat |
| PASTI-NUM-003 | Frasa batas yang nggak jelas ketat atau inklusif (`di atas 80`, `minimal 5`) | kandidat |
| PASTI-NUM-004 | Rentang tanpa keterangan inklusif atau satuan (`10–20 °C`). Rujukan kayak `langkah 4–6` di-skip. | kandidat |
| PASTI-GRA-001 | Penulisan imbuhan dan kata depan dari list yang nggak punya tafsir lain (`disini`, `di pasang`), plus pasangan dari project | violation |
| PASTI-GRA-011 | Bentuk ulang yang ditulis `kabel2` atau `baut baut` | kandidat |
| PASTI-BAS-003 | Status istilah di glossary: deprecated, restricted, proposed (butuh glossary) | violation, kandidat, atau `not_evaluated` |
| PASTI-LEX-005 | Satu bentuk kata yang nyambung ke beberapa konsep di glossary | kandidat |
| PASTI-LEX-007 | Konflik antar-file glossary. Tool ini nggak pakai aturan "file terakhir menang". | violation |
| PASTI-SEM-001..004, NUM-006 | Perubahan makna antara sumber dan revisi (`pasti compare`) | kandidat aja |
| PASTI-FOR-001..006 | Profil formal EX01 (`pasti parse`) | diterima, atau ditolak dengan reason code |

Rule wajib yang butuh penilaian manusia masuk ke `coverage.mandatory_rules_requiring_manual_review`, misalnya pelaku dan rujukan (BAS-002, BAS-007), satu tindakan per kalimat (BAS-004), atau bentuk imperatif (GRA-003). Rule-rule ini tetap muncul di report.

### Cara ngitung kata (Dok. 06)

Satu unit = token yang dipisah whitespace (termasuk NBSP dan narrow NBSP) dan isinya ada huruf, angka, atau simbol satuan. Contohnya:

- `sensor-sensor`, `katupnya`, `2,5`, dan `80°C` masing-masing 1 unit, sedangkan `80 °C` dihitung 2.
- Inline code, inline math, URL, dan gambar masing-masing dihitung 1 atom.
- Tanda baca atau operator yang berdiri sendiri (`>`, `±`, `–`) nggak dihitung.
- Code block, kutipan `exact`, dan persamaan display itu protected. Report tetap nyatet blok-blok ini, tapi rule prosa nggak jalan di situ.

Kalimat nggak kepotong di desimal, versi, elipsis, atau singkatan yang terdaftar. Daftar singkatannya bisa kamu atur di `abbreviations`.

## Config

```yaml
config_version: 1
spec_version: 0.1.0-draft.1
profile_id: documentation
profile_version: 0.1.0-draft.1
default_mode: DESC
length_review: { PROC: 20, WARN: 20, DESC: 25, REQ: 25, UI: null }
gate:
  fail_on: error                    # error | warning | info
  mandatory_checks_complete: false  # true: gagal selama masih ada review pending atau cek yang belum jalan
  # scope: [PASTI-TOK-005, ...]     # default: semua rule teks otomatis (rule glossary cuma kalau ada glossary)
fix: { automatic: false }           # autofix belum ada; kalau diisi true bakal ditolak
glossaries: [glossary-seed.json]
reviews: reviews.yaml
spelling_pairs: [{ wrong: dipanel, right: di panel }]
defined_comparisons: [minimal]      # frasa batas yang udah didefinisikan di profil
disabled_rules: []                  # tetap dilaporin not_evaluated, nggak disembunyiin
```

Urutan merge-nya: profil bawaan, lalu file project (`--config`, atau `./pasti.yaml` kalau ada), lalu flag CLI. Report JSON nyatet config efektif, hash glossary dan source, sama versi tool.

## Glossary

Format file glossary: `{"glossary_id", "glossary_version", "entries": [...]}`, tiap entry ikut schema Dok. 05. Entry yang nggak valid bikin loading gagal dengan exit code 2, misalnya status approved tanpa `review_record_id`, atau restricted tanpa constraints.

`examples/glossary-seed.json` isinya 21 konsep seed dari Dok. 05, semuanya masih `proposed`. Istilah yang nggak ada di glossary nggak pernah dianggap pelanggaran.

## Keputusan manusia: review, exception, deviation, suppression

Tool ini nggak nulis keputusan sendiri. Kamu yang nyatet keputusan di file YAML, terikat ke rule dan teks persis di scope-nya. Ambil hash-nya dari bagian setelah `:` di `scope_id` diagnostic:

```yaml
records:
  - record_id: R-1
    kind: review          # review | exception | deviation | suppression
    state: accepted       # proposed | accepted | rejected
    rule_id: PASTI-TOK-005
    scope_hash: 3f1c2a9b0d4e5f61
    decision: satisfied   # wajib buat review
    reason: Pemecahan kalimat akan memisahkan syarat dari tindakan.
    reviewed_by: [Nama (penelaah bahasa)]
```

Efek tiap `kind`:

- `review` nutup review pakai keputusan yang dicatat.
- `exception` wajib punya `basis` (klausul yang ngizinin), dan bikin scope-nya `satisfied`.
- `deviation` tetap nyatet pelanggarannya, tapi `fail_on` di gate nggak ngitung lagi.
- `suppression` cuma nyembunyiin dari tampilan. Coverage dan gate tetap ngitung.

Kalau teksnya kamu edit, record-nya jadi stale. Report bakal ngedaftar record stale dan nggak nerapin record itu.

## Profil formal EX01

`examples/registry-demo.json` itu registry demo dari Dok. 10. Kalimat yang diterima ngehasilin AST sesuai Dok. 10. Kalimat yang ditolak cuma dapat reason code: `FOR_SYNTAX`, `FOR_UNKNOWN_REFERENCE`, `FOR_CAPABILITY_MISMATCH`, `FOR_UNIT_MISMATCH`, atau `FOR_OWNER_MISMATCH`. Parser-nya nggak konversi satuan dan nggak ngeksekusi apa pun.

## Development

```sh
cargo test     # unit test + fixture Dok. 09 F01-F27
cargo clippy --all-targets
```

Source-nya di `src/`:

- `document.rs`: adapter Markdown dan text, lengkap dengan source map.
- `segment.rs`: pemecahan kalimat dan penghitungan unit.
- `patterns.rs`: detector-detector-nya.
- `checks.rs`: hasil per scope.
- `report.rs`: coverage dan gate.
- `reviews.rs`: record keputusan.
- `glossary.rs`, `compare.rs`, `formal.rs`.

Buat release, push tag `v*`. Workflow `.github/workflows/release.yml` bakal build wheel (Linux glibc/musl x86_64/aarch64, macOS, Windows) pakai maturin, terus publish ke PyPI lewat trusted publishing.

## License

Belum ada license, karena kebijakan lisensinya masih dibahas di draft tata kelola PASTI (Dok. 11). Sampai license-nya ditambahin, semua hak dilindungi (all rights reserved).
