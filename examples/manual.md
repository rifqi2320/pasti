# Penggantian sensor tekanan

<!-- pasti: mode=DESC -->

Sensor tekanan berada pada rangka utama. Pengendali membaca sinyal sensor setiap siklus.

<!-- pasti: mode=WARN -->

Permukaan panas dapat menyebabkan luka bakar. Jangan sentuh permukaan sebelum suhu kurang dari 40 °C.

<!-- pasti: mode=PROC -->

1. Matikan pompa utama dan tunggu minimal 5 menit.
2. Periksa setiap baut pada penutup pompa utama dengan kunci yang sesuai sebelum teknisi lain memasang kembali pelindung pada rangka mesin itu.
3. Lepas sensor yang di pasang disini.
4. Atur tekanan ke 2.5 bar dan jaga suhu 10–20 °C.
5. Jalankan `tool check --all`.

<!-- pasti: exact -->

> Klik Restart.

```sh
tool check --all --verbose
```
