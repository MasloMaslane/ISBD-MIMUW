# Projekt 2 – format pliku

Budowanie: `make all`

```
isbd gen <plik> <wiersze>   # generuje plik (INT64, BOOLEAN, 2x VARCHAR)
isbd stats <plik>           # srednia / entropia / liczba znakow ASCII
```

`make sample` generuje `sample.isbd` (10000 wierszy), `make stats` liczy dla niego metryki, `make test` uruchamia testy.

## Format

Little-endian.

```
"ISBD" | wersja (u8) | grupa 0 | grupa 1 | ... | stopka | dlugosc stopki (u64) | "ISBD"
```

Grupa wierszy to maksymalnie `BATCH_SIZE` (1024) wierszy, po kolei chunk kazdej kolumny. Jedna grupa jest czytana jako jeden `Batch`.

Chunki:
- INT64: delty od poprzedniej wartosci (pierwsza od 0), zigzag, varint (LEB128)
- BOOLEAN: 1 bit na wartosc, `ceil(n/8)` bajtow
- VARCHAR: dlugosci napisow jako varinty + bajty napisow, calosc skompresowana LZ4

Stopka (liczby jako varinty): liczba kolumn, dla kazdej nazwa (dlugosc + bajty) i typ (u8: 0 INT64, 1 BOOLEAN, 2 VARCHAR); liczba grup, dla kazdej liczba wierszy i (offset, dlugosc) chunka kazdej kolumny.
