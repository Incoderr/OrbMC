<p align="center"><img src="assets/logo/orbmc.svg" width="128" alt="OrbMC"></p>

<h1 align="center">OrbMC</h1>

<p align="center">Русский · <a href="README.en.md">English</a></p>

[![Checks](https://github.com/Incoderr/OrbMC/actions/workflows/checks.yml/badge.svg)](https://github.com/Incoderr/OrbMC/actions/workflows/checks.yml)
[![Release](https://img.shields.io/github/v/release/Incoderr/OrbMC?label=релиз)](https://github.com/Incoderr/OrbMC/releases/latest)
[![License: GPL-3.0](https://img.shields.io/badge/лицензия-GPL--3.0-blue.svg)](LICENSE)
![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)
![Windows](https://img.shields.io/badge/платформа-Windows-0078D6.svg)

OrbMC — быстрый и нативный лаунчер Minecraft написанный на Rust. Интерфейс построен на
[GPUI](https://www.gpui.rs/), без Electron, WebView и Node.js. Java требуется только самой игре;
лаунчер устанавливает нужную версию автоматически.

## Установка

Загрузите `orbmc.exe` со страницы
[последнего релиза](https://github.com/Incoderr/OrbMC/releases/latest)
и запустите его. Установщик не требуется.

## Сборка из исходников

Требуется стабильный Rust 1.85 или новее.

```bash
cargo build --release
```

Отладочная сборка (`cargo run`) включает инспектор GPUI; флаг
`--no-default-features` отключает его.

Необязательные переменные времени сборки:

| Переменная                | Назначение                                                                  |
| ------------------------- | --------------------------------------------------------------------------- |
| `ORBMC_DISCORD_APP_ID`    | ID приложения Discord для отображения статуса                               |
| `ORBMC_UPDATE_PUBLIC_KEY` | публичный ключ minisign; без него лаунчер только уведомляет о новых версиях |
| `CURSEFORGE_API_KEY`      | ключ CurseForge API (интеграция с CurseForge сейчас отключена)              |

Релиз выпускается тегом `vX.Y.Z`, совпадающим с версией в `Cargo.toml`;
workflow `release.yml` собирает, подписывает и публикует исполняемый файл.

## Данные

| Содержимое                                | Расположение                                   |
| ----------------------------------------- | ---------------------------------------------- |
| Настройки, список сборок, служебные файлы | `%LOCALAPPDATA%\OrbMC`                         |
| Журнал лаунчера                           | `%LOCALAPPDATA%\OrbMC\logs\launcher.log`       |
| Файлы сборок                              | `<папка загрузок>\instances\<название сборки>` |

При сообщении об ошибке приложите `launcher.log`.

## Разработка

```text
app/     приложение: фичи, интерфейс, интеграция с ОС
core/    логика Minecraft без интерфейса
vendor/  изменённые копии зависимостей (см. vendor/README.md)
```

Архитектура и правила кода описаны в [`AGENTS.md`](AGENTS.md). Перед коммитом:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Лицензия

[GPL-3.0](LICENSE). Зависимости в `vendor/` распространяются под собственными
лицензиями. Minecraft — товарный знак Mojang AB; проект не связан с Mojang и Microsoft.
