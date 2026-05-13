[![Boosty](https://img.shields.io/badge/Boosty-Buy_me_a_coffee-FF7143?logo=boosty&logoColor=white&style=for-the-badge)](https://boosty.to/danusha/donate)

# ASIC Discover

## English

ASIC Discover is a cross-platform Rust utility for finding ASIC miners on local IPv4 networks.

It does not change miner settings and does not brute-force passwords. The scanner checks common ASIC management ports, CGMiner/BMMiner/BOSMiner-compatible APIs, and HTTP management pages. Results are printed to the console and saved to `reports/*.json`, `reports/*.csv`, and the local inventory database `database/asic_inventory.jsonl`.

### Features

- Finds ASIC miner candidates on local IPv4 networks.
- Supports common ASIC fingerprints: Antminer, WhatsMiner, Avalon, Goldshell, IceRiver, Braiins OS, VNish, Hiveon ASIC, Innosilicon, Jasminer, DragonMint, Baikal, iBeLink, StrongU, Ebang Ebit, Dayun, BlackMiner.
- Reads CGMiner/BMMiner/BOSMiner API data from ports `4028` and `4029`.
- Extracts telemetry when available: hashrate, temperatures, and fan RPM.
- Provides watch mode with a static table that redraws only when something changes.
- Includes ready-to-run binaries for Windows and Linux x64/ARM targets.
- Builds without external Rust crate dependencies.

### Quick Start On Windows

Clone or download the repository, then open the project folder:

```powershell
git clone https://github.com/danusha2345/asic-discover.git
cd asic-discover
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1
```

Ready Windows binaries:

```text
bin\asic-discover.exe
dist\x86_64-pc-windows-msvc\asic-discover.exe
```

Scan a specific network:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --deep
```

Run continuously and update the same table every 30 seconds only when results change:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --deep --watch --interval 30
```

Or through the PowerShell launcher:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1 -Watch -Interval 30
```

### Quick Start On Linux

```bash
git clone https://github.com/danusha2345/asic-discover.git
cd asic-discover
chmod +x ./run_scan.sh
./run_scan.sh
```

Scan a specific network:

```bash
./run_scan.sh --network 192.168.1.0/24 --deep
```

Run in watch mode:

```bash
./run_scan.sh --watch --interval 30 --network 192.168.1.0/24 --deep
```

`run_scan.sh` detects the Linux architecture and tries to use one of the ready binaries:

```text
dist/x86_64-unknown-linux-musl/asic-discover
dist/aarch64-unknown-linux-musl/asic-discover
dist/armv7-unknown-linux-musleabihf/asic-discover
```

If no matching binary is found but Cargo is installed, the launcher builds and runs:

```bash
cargo build --release
./target/release/asic-discover
```

### Useful Commands

List auto-detected networks:

```powershell
.\bin\asic-discover.exe --list-networks
```

Show low-confidence candidates too:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --include-low
```

Scan custom ports:

```powershell
.\bin\asic-discover.exe --network 10.10.0.0/22 --ports 22,23,80,443,4028,4029,8080,8888 --force
```

Use HTTP Basic Auth for miner web UIs:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --user root --password root
```

Disable database writes:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --no-db
```

Use a custom database path:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --database D:\asic_inventory.jsonl
```

### Speed Tuning

The scanner uses a two-stage pipeline:

1. Fast parallel TCP probing of all selected `IP:port` pairs.
2. Detailed API/HTTP fingerprinting only for hosts with open ports.

Default concurrency is `512` worker threads. On large local networks you can make scans faster by increasing concurrency and lowering the TCP timeout:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --threads 1024 --timeout 0.25
```

For unstable Wi-Fi, routed networks, or slow devices, keep a safer timeout:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --threads 512 --timeout 0.55
```

### Output

Example table:

```text
IP            CONF    SCORE  VENDOR / MODEL             HASHRATE    TEMP C            FAN RPM    PORTS      REASON
------------  ------  -----  -------------------------  ----------  ----------------  ---------  ---------  --------------------------------------------
192.168.1.41  high    100    Bitmain Antminer S19       104.5 TH/s  max 72; 68,70,72  6150,6200  80,4028   CGMiner-compatible API answered
192.168.1.42  high    100    Bitmain Antminer S21       188.2 TH/s  max 69; 64,66,69  5520,5590  80,4028   vendor fingerprint: Bitmain Antminer
192.168.1.43  high    96     MicroBT WhatsMiner M50     118.7 TH/s  max 76; 72,76     5750,5800  80,4028   vendor fingerprint: MicroBT WhatsMiner
192.168.1.44  high    91     Canaan Avalon A1366        129.4 TH/s  max 71; 66,69,71  5000,5100  80,4028   mining terms: hashrate, fan, pool
192.168.1.45  high    88     Goldshell KD6              26.3 TH/s   max 64; 60,64     4320,4400  80,8080   HTTP fingerprint on 80/
192.168.1.46  high    84     IceRiver KS3M              6.1 TH/s    max 67; 63,67     3900,4020  80,4028   CGMiner-compatible API answered
192.168.1.47  medium  68     Braiins OS                 96.8 TH/s   max 70; 65,70     6100,6180  80,4028   vendor fingerprint: Braiins OS
192.168.1.48  medium  61     VNish                      82.4 TH/s   max 73; 69,73     5950,6010  80,4028   authenticated miner web UI
192.168.1.49  medium  57     Innosilicon A11            1.5 TH/s    max 62; 59,62     3200,3300  80,8080   HTTP fingerprint on 8080/
```

### Inventory Database

Each scan can append discovered devices to:

```text
database/asic_inventory.jsonl
```

This is an append-only JSONL database: one JSON line per discovered device per changed scan result. The latest scan table is also saved to:

```text
database/latest_inventory.csv
```

Records include IP, vendor, model, confidence, open ports, hashrate, temperatures, fan RPM, and detection reasons.

### Build For Multiple Systems

On Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -KeepGoing
```

Install missing Rust targets and build:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -InstallTargets -KeepGoing
```

On Linux:

```bash
chmod +x ./build_all.sh
./build_all.sh --keep-going
```

Artifacts are written to:

```text
dist/<target>/asic-discover
dist/<target>/asic-discover.exe
dist/SHA256SUMS.txt
```

Main targets:

- `x86_64-pc-windows-msvc` - Windows x64.
- `x86_64-unknown-linux-musl` - Linux x64 static binary.
- `aarch64-unknown-linux-musl` - Linux ARM64.
- `armv7-unknown-linux-musleabihf` - Linux ARMv7.
- `x86_64-pc-windows-gnu` - Windows x64 from Linux through MinGW.

### Limitations

No scanner can detect every ASIC in every environment. Devices can be in another VLAN, behind a firewall, use a disabled web UI, or listen on custom ports. Use `--network` and `--ports` when needed.

This build intentionally avoids external Rust crates. It detects open `443`, but it does not perform full HTTPS/TLS page parsing. Most devices are still detected through HTTP and CGMiner-compatible APIs.

Only scan networks you own or are allowed to audit.

---

## Русский

ASIC Discover - кроссплатформенная Rust-утилита для поиска ASIC-майнеров в локальных IPv4-сетях.

Сканер не меняет настройки устройств и не подбирает пароли. Он проверяет типовые порты управления ASIC, CGMiner/BMMiner/BOSMiner-совместимые API и HTTP-страницы управления. Результаты выводятся в консоль и сохраняются в `reports/*.json`, `reports/*.csv` и локальную базу `database/asic_inventory.jsonl`.

### Возможности

- Поиск ASIC-кандидатов в локальных IPv4-сетях.
- Сигнатуры популярных ASIC: Antminer, WhatsMiner, Avalon, Goldshell, IceRiver, Braiins OS, VNish, Hiveon ASIC, Innosilicon, Jasminer, DragonMint, Baikal, iBeLink, StrongU, Ebang Ebit, Dayun, BlackMiner.
- Чтение CGMiner/BMMiner/BOSMiner API на портах `4028` и `4029`.
- Извлечение телеметрии, если устройство её отдаёт: хэшрейт, температуры, обороты вентиляторов.
- Watch-режим со статичной таблицей, которая перерисовывается только при изменениях.
- Готовые бинарники для Windows и Linux x64/ARM.
- Сборка без внешних Rust crate-зависимостей.

### Быстрый Запуск На Windows

Склонируйте или скачайте репозиторий, затем откройте папку проекта:

```powershell
git clone https://github.com/danusha2345/asic-discover.git
cd asic-discover
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1
```

Готовые Windows-бинарники:

```text
bin\asic-discover.exe
dist\x86_64-pc-windows-msvc\asic-discover.exe
```

Скан конкретной сети:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --deep
```

Непрерывный режим с обновлением текущей таблицы каждые 30 секунд только при изменениях:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --deep --watch --interval 30
```

Или через PowerShell launcher:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\run_scan.ps1 -Watch -Interval 30
```

### Быстрый Запуск На Linux

```bash
git clone https://github.com/danusha2345/asic-discover.git
cd asic-discover
chmod +x ./run_scan.sh
./run_scan.sh
```

Скан конкретной сети:

```bash
./run_scan.sh --network 192.168.1.0/24 --deep
```

Watch-режим:

```bash
./run_scan.sh --watch --interval 30 --network 192.168.1.0/24 --deep
```

`run_scan.sh` сам определяет архитектуру Linux и ищет готовый бинарник:

```text
dist/x86_64-unknown-linux-musl/asic-discover
dist/aarch64-unknown-linux-musl/asic-discover
dist/armv7-unknown-linux-musleabihf/asic-discover
```

Если готового бинарника нет, но установлен Cargo, launcher соберёт и запустит:

```bash
cargo build --release
./target/release/asic-discover
```

### Полезные Команды

Показать автоматически найденные локальные подсети:

```powershell
.\bin\asic-discover.exe --list-networks
```

Показать даже слабые совпадения:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --include-low
```

Скан нестандартных портов:

```powershell
.\bin\asic-discover.exe --network 10.10.0.0/22 --ports 22,23,80,443,4028,4029,8080,8888 --force
```

HTTP Basic Auth для веб-интерфейсов майнеров:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --user root --password root
```

Отключить запись в базу:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --no-db
```

Указать другой путь к базе:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --database D:\asic_inventory.jsonl
```

### Настройка Скорости

Сканер работает в два этапа:

1. Быстрая параллельная TCP-проверка всех выбранных пар `IP:port`.
2. Детальный API/HTTP-fingerprint только для хостов, где есть открытые порты.

По умолчанию используется `512` worker threads. На больших локальных сетях можно ускорить скан, подняв конкуррентность и уменьшив TCP timeout:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --threads 1024 --timeout 0.25
```

Для нестабильного Wi-Fi, маршрутизируемых сетей или медленных устройств лучше оставить более безопасный timeout:

```powershell
.\bin\asic-discover.exe --network 192.168.1.0/24 --threads 512 --timeout 0.55
```

### Вывод

Пример таблицы:

```text
IP            CONF    SCORE  VENDOR / MODEL             HASHRATE    TEMP C            FAN RPM    PORTS      REASON
------------  ------  -----  -------------------------  ----------  ----------------  ---------  ---------  --------------------------------------------
192.168.1.41  high    100    Bitmain Antminer S19       104.5 TH/s  max 72; 68,70,72  6150,6200  80,4028   CGMiner-compatible API answered
192.168.1.42  high    100    Bitmain Antminer S21       188.2 TH/s  max 69; 64,66,69  5520,5590  80,4028   vendor fingerprint: Bitmain Antminer
192.168.1.43  high    96     MicroBT WhatsMiner M50     118.7 TH/s  max 76; 72,76     5750,5800  80,4028   vendor fingerprint: MicroBT WhatsMiner
192.168.1.44  high    91     Canaan Avalon A1366        129.4 TH/s  max 71; 66,69,71  5000,5100  80,4028   mining terms: hashrate, fan, pool
192.168.1.45  high    88     Goldshell KD6              26.3 TH/s   max 64; 60,64     4320,4400  80,8080   HTTP fingerprint on 80/
192.168.1.46  high    84     IceRiver KS3M              6.1 TH/s    max 67; 63,67     3900,4020  80,4028   CGMiner-compatible API answered
192.168.1.47  medium  68     Braiins OS                 96.8 TH/s   max 70; 65,70     6100,6180  80,4028   vendor fingerprint: Braiins OS
192.168.1.48  medium  61     VNish                      82.4 TH/s   max 73; 69,73     5950,6010  80,4028   authenticated miner web UI
192.168.1.49  medium  57     Innosilicon A11            1.5 TH/s    max 62; 59,62     3200,3300  80,8080   HTTP fingerprint on 8080/
```

### База

Каждый скан может дописывать найденные устройства в:

```text
database/asic_inventory.jsonl
```

Это append-only JSONL-база: одна строка JSON на одно найденное устройство за один изменившийся результат скана. Последняя таблица также сохраняется в:

```text
database/latest_inventory.csv
```

Записи содержат IP, производителя, модель, уровень уверенности, открытые порты, хэшрейт, температуры, вентиляторы и причины определения.

### Сборка Под Разные Системы

На Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -KeepGoing
```

Установить недостающие Rust targets и собрать:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\build_all.ps1 -InstallTargets -KeepGoing
```

На Linux:

```bash
chmod +x ./build_all.sh
./build_all.sh --keep-going
```

Артефакты складываются в:

```text
dist/<target>/asic-discover
dist/<target>/asic-discover.exe
dist/SHA256SUMS.txt
```

Основные targets:

- `x86_64-pc-windows-msvc` - Windows x64.
- `x86_64-unknown-linux-musl` - Linux x64, статический бинарник.
- `aarch64-unknown-linux-musl` - Linux ARM64.
- `armv7-unknown-linux-musleabihf` - Linux ARMv7.
- `x86_64-pc-windows-gnu` - Windows x64 из Linux через MinGW.

### Ограничения

Ни один сканер не может гарантированно найти все ASIC в любой сети. Устройства могут быть в другой VLAN, за фаерволом, с отключенным веб-интерфейсом или на нестандартных портах. В таких случаях используйте `--network` и `--ports`.

Эта сборка намеренно не использует внешние Rust crates. Она видит открытый `443`, но не делает полноценный HTTPS/TLS-разбор страниц. Большинство устройств всё равно определяется через HTTP и CGMiner-совместимые API.

Сканируйте только свои сети или сети, для которых у вас есть разрешение.
