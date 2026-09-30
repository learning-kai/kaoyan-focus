# Notice and asset attribution

This project is distributed under the MIT License. Source code licensing is covered by [LICENSE](./LICENSE).

Bundled non-code assets need explicit provenance before public releases. Do not assume that a source asset is covered by the repository license unless it is listed here.

## Project assets

| Asset                             | Purpose                           | Source / owner     | License status                                   |
| --------------------------------- | --------------------------------- | ------------------ | ------------------------------------------------ |
| `src-tauri/icons/icon.ico`        | Windows application icon          | Project maintainer | To be confirmed before broad public distribution |
| `BGM/Summer Baby（夏日心动）.mp3` | Reminder / background audio asset | To be confirmed    | To be confirmed before broad public distribution |

## Ambient sounds (白噪音)

Files under `public/sounds/ambient/` are bundled into the app and play in the 白噪音 mixer. They are the loop-edited AAC versions distributed by the [Blankie](https://blankie.rest/credits) project ([source repository](https://github.com/codybrom/blankie), `Blankie/Resources/Sounds/`), retrieved 2026-09-30. Changes from the originals: trimmed into seamless loops and re-encoded as AAC (`.m4a`). The same credits are shown in the app under 白噪音 → 音源署名与协议.

| Asset                                    | Title        | Author         | Source                                                      | License                                                   |
| ---------------------------------------- | ------------ | -------------- | ----------------------------------------------------------- | --------------------------------------------------------- |
| `public/sounds/ambient/rain.m4a`         | Rain         | alex36917      | https://freesound.org/s/524605/                             | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |
| `public/sounds/ambient/storm.m4a`        | Storm        | Digifish music | https://freesound.org/s/41739                               | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |
| `public/sounds/ambient/waves.m4a`        | Waves        | Luftrum        | https://freesound.org/s/48412/                              | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |
| `public/sounds/ambient/train.m4a`        | Train        | SDLx           | https://freesound.org/s/259988/                             | [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/) |
| `public/sounds/ambient/city.m4a`         | City         | gezortenplotz  | https://freesound.org/s/44796/                              | [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/) |
| `public/sounds/ambient/wind.m4a`         | Wind         | felix.blume    | https://freesound.org/s/217506/                             | [CC0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `public/sounds/ambient/stream.m4a`       | Stream       | gluckose       | https://freesound.org/s/333987/                             | [CC0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `public/sounds/ambient/birds.m4a`        | Birds        | kvgarlic       | https://freesound.org/s/156826/                             | [CC0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `public/sounds/ambient/boat.m4a`         | Boat         | Falcet         | https://freesound.org/s/439365/                             | [CC0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `public/sounds/ambient/summer-night.m4a` | Summer Night | Lisa Redfern   | https://soundbible.com/2083-Crickets-Chirping-At-Night.html | Public Domain                                             |
| `public/sounds/ambient/coffee-shop.m4a`  | Coffee Shop  | stephan        | https://soundbible.com/1664-Restaurant-Ambiance.html        | Public Domain                                             |
| `public/sounds/ambient/fireplace.m4a`    | Fireplace    | ezwa           | https://soundbible.com/1543-Fireplace.html                  | Public Domain                                             |
| `public/sounds/ambient/pink-noise.m4a`   | Pink Noise   | Blankie        | https://blankie.rest/credits                                | [CC0](https://creativecommons.org/publicdomain/zero/1.0/) |
| `public/sounds/ambient/white-noise.m4a`  | White Noise  | Blankie        | https://blankie.rest/credits                                | [CC0](https://creativecommons.org/publicdomain/zero/1.0/) |

## Policy

- New screenshots, GIFs, diagrams and generated UI assets should be created from test data and stored under `docs/assets/`.
- Third-party audio, icons, fonts, templates or illustrations must include source URL, author, license name and retrieval date.
- If an asset license is unclear, replace it with a self-owned or clearly licensed alternative before publishing releases.
- Secrets, databases, logs, sync backups and private keys must never be committed as assets.
