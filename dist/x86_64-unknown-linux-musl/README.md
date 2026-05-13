[![Boosty](https://img.shields.io/badge/Boosty-Buy_me_a_coffee-FF7143?logo=boosty&logoColor=white&style=for-the-badge)](https://boosty.to/danusha/donate)

# ASIC Discover

Rust-утилита для поиска ASIC-майнеров в локальной IPv4-сети.

Сканер не меняет настройки устройств и не подбирает пароли. Он только проверяет
типовые порты управления ASIC, CGMiner/BMMiner/BOSMiner API и HTTP-интерфейсы,
после чего сохраняет найденные кандидаты в `reports/*.json`, `reports/*.csv`
и локальную базу `database/asic_inventory.jsonl`.

## Быстрый запуск на Windows

```powershell
cd "O:\Новая папка\asic_утилита"
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1
```

Если рядом есть готовый `bin\asic-discover.exe`, запустится он. Если бинарника
нет, launcher соберет проект через Cargo:

```powershell
cargo run --release --
```

В Windows готовый exe уже лежит здесь:

```text
bin\asic-discover.exe
dist\x86_64-pc-windows-msvc\asic-discover.exe
```

Непрерывный режим с обновлением текущей таблицы:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1 -Watch -Interval 30
```

В этом режиме скан повторяется по интервалу, но экран перерисовывается только
если изменился список найденных ASIC или отображаемые параметры строк.

## Быстрый запуск на Linux

Самый простой запуск:

```bash
cd /path/to/asic_утилита
chmod +x ./run_scan.sh
./run_scan.sh
```

Скан конкретной сети:

```bash
./run_scan.sh --network 192.168.1.0/24 --deep
```

Непрерывный режим:

```bash
./run_scan.sh --watch --interval 30 --network 192.168.1.0/24 --deep
```

`run_scan.sh` сам определяет архитектуру Linux и ищет готовый бинарник:

```text
dist/x86_64-unknown-linux-musl/asic-discover
dist/aarch64-unknown-linux-musl/asic-discover
dist/armv7-unknown-linux-musleabihf/asic-discover
```

Если готового бинарника нет, но установлен Cargo, скрипт сам выполнит:

```bash
cargo build --release
```

и запустит `target/release/asic-discover`.

## Сборка под разные системы

Проект не использует внешние Rust crates, поэтому его удобно собирать под
разные target-платформы через стандартный `cargo build --target`.

На Windows:

```powershell
cd "O:\Новая папка\asic_утилита"
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -KeepGoing
```

С установкой недостающих Rust targets:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -InstallTargets -KeepGoing
```

Собрать только конкретную платформу:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -Targets x86_64-pc-windows-msvc
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -Targets x86_64-unknown-linux-musl
```

На Linux:

```bash
cd /path/to/asic_утилита
chmod +x ./build_all.sh
./build_all.sh --keep-going
```

Собрать конкретный target:

```bash
./build_all.sh --target x86_64-unknown-linux-musl
./build_all.sh --target aarch64-unknown-linux-musl
./build_all.sh --target x86_64-pc-windows-gnu
```

Готовые файлы складываются в:

```text
dist/<target>/asic-discover
dist/<target>/asic-discover.exe
dist/SHA256SUMS.txt
```

Основные target'ы:

- `x86_64-pc-windows-msvc` - Windows x64;
- `x86_64-unknown-linux-musl` - Linux x64, статическая сборка;
- `aarch64-unknown-linux-musl` - Linux ARM64;
- `armv7-unknown-linux-musleabihf` - Linux ARMv7;
- `x86_64-pc-windows-gnu` - Windows x64 из Linux через MinGW.

Если target не установлен:

```powershell
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
```

Для `linux-gnu` и `windows-gnu` targets может понадобиться внешний cross-linker.
Для переносимых Linux-бинарников проще использовать `*-unknown-linux-musl`.

## Команды

Показать автоматически найденные локальные подсети:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1 -ListNetworks
```

Просканировать конкретную подсеть:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24
```

Глубже проверить веб-интерфейсы:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --deep
```

Показать даже слабые совпадения:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --include-low
```

Если майнеры используют нестандартные порты:

```powershell
.\bin\asic-discover.exe --network 10.10.0.0/22 --ports 22,23,80,443,4028,4029,8080,8888 --force
```

Оставить утилиту запущенной и пересканировать сеть каждые 30 секунд:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --deep --watch --interval 30
```

В watch-режиме вывод статичный: утилита не печатает новую таблицу каждую
итерацию, а очищает экран и перерисовывает текущие строки только при изменениях.
Остановить можно через `Ctrl+C`.

Если веб-интерфейс закрыт Basic Auth:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --user root --password root
```

Отключить запись в базу, оставив только отчеты:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --no-db
```

Записать базу в другой файл:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --database D:\asic_inventory.jsonl
```

## Что определяется

Утилита повышает уверенность, если видит признаки:

- открытый API-порт `4028` или `4029`;
- ответ CGMiner/BMMiner/BOSMiner API;
- веб-страницы или HTTP-заголовки с `Antminer`, `WhatsMiner`, `Avalon`,
  `Goldshell`, `IceRiver`, `Braiins OS`, `VNish`, `Hiveon ASIC` и похожими
  сигнатурами;
- поля статуса вроде hashrate, pool, fan speed, temperature, chain.

Если устройство отдает телеметрию, в консоли и отчетах появляются:

- `HASHRATE` - текущий или средний хэшрейт, нормализованный в `TH/s`;
- `TEMP C` - найденные температуры и максимум;
- `FAN RPM` - скорости вентиляторов в оборотах в минуту.

## База

При каждом скане утилита дописывает найденные устройства в:

```text
database/asic_inventory.jsonl
```

Это append-only база: одна строка JSON на одно найденное устройство за один
скан. Для быстрого просмотра последнего скана дополнительно создается:

```text
database/latest_inventory.csv
```

Обе записи содержат IP, производителя, модель, уровень уверенности, открытые
порты, хэшрейт, температуры и вентиляторы.

Уровни:

- `high` - почти наверняка ASIC-майнер;
- `medium` - очень похож на ASIC, но без полной идентификации;
- `low` - слабое совпадение, по умолчанию скрывается.

## Ограничения

100% универсального определения всех ASIC не существует: часть устройств может
быть в другой VLAN, за фаерволом, с отключенным веб-интерфейсом или нестандартным
портом. В таком случае укажите правильную подсеть через `--network` и добавьте
порты через `--ports`.

Версия без внешних Rust crate-зависимостей не делает полноценный TLS-handshake
для HTTPS-страниц, но учитывает открытый `443` и определяет большинство ASIC по
HTTP и CGMiner-совместимому API.

Не запускайте сканирование чужих сетей без разрешения.
