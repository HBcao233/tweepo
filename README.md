# tweepo

Internationalization for `.twee`

## Usage

```
# gettext .twee -> .pot
tweego gettext Start.twee -o locales/en/Start.pot

# gettext recursively
tweego gettext -r src/ -o locales/en/

# init .po from .pot
msginit -i locales/en/Start.pot -o locales locales/zh_CN/Start.po

# update .po from .pot
msgupdate -i locales/en/Start.pot -o locales locales/zh_CN/Start.po

# generate translated .twee from .po
tweego apply src/Start.twee locales/zh_CN/Start.po -o dist-zh_CN/Start.twee

# generate recursively (Traverse all .twee files in the src/, and use same name .po file in locales/zh_CN/ to generate .twee)
tweego apply -r src/ locales/zh_CN/ -o dist-zh_CN

# check only
tweego check src/ locales/zh_CN
```

## License

All the libraries and binaries contained in this repository are licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)

* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
