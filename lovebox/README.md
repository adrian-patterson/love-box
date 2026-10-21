# Renderer

From the repository root, `make render-samples` generates a synthetic photo and one PNG
per card in `out/`. No hardware or network is needed.

```sh
cargo run --manifest-path lovebox/Cargo.toml -- fonts
cargo run --manifest-path lovebox/Cargo.toml -- render message --size 48 'Hello, love'
cargo run --manifest-path lovebox/Cargo.toml -- render quote --file quote.txt --align center -o out/quote.png
cargo run --manifest-path lovebox/Cargo.toml -- render photo photo.jpg --dither atkinson --focal 0.5,0.3 --keep-copy out/copy.jpg
```

Output defaults to `out/frame.png` relative to the working directory. Quotes accept at
most 2,000 normalised characters, with 40 px margins and a 14 px minimum; text that cannot
fit is rejected. `render clear` writes an all-white frame.

- `frame`: packed 800x480 pixels, MSB first, 1 = white; exact 1-bit PNG encoding.
- `panel`: `Panel` interface and `PngPanel` (current frame plus 20 history files).
- `fonts`: seven bundled OFL faces across four families.
- `render`: pure card rendering; message layout, quote auto-fit, photo crop and dithering.
- `render::photo`: `ingest` at upload time, then `frame` from the bounded RGB copy.
  JPEG uses reduced IDCT decoding; other formats decode before downscaling. `IngestLimits`
  checks dimensions and budgets decode buffers; codec scratch/allocator overhead means
  `max_alloc` is not a hard process RSS ceiling. The CLI exposes `--max-alloc-mib 160`.

Run `make test` and `make lint`. Regenerate intended goldens from this directory with
`UPDATE_GOLDENS=1 cargo test --test golden`, then inspect the changed PNGs before committing.
Large fixtures are generated, never checked in: `cargo run --example photo-fixture --
../out/40mp.jpg 8000 5000` (use `.tiff` for RGB16).
