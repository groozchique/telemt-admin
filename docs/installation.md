# Установка и обновление

Требования, установка (скрипт, Docker, вручную), проверка после запуска, обновление и сборка из исходников. Настройки — в [configuration.md](configuration.md).

## Требования

- Linux-сервер с `systemd`.
- `root`-доступ или `sudo`.
- Telegram-бот и токен от [@BotFather](https://t.me/BotFather).
- Telegram user ID администраторов (можно получить через `@userinfobot`).
- Публичный IPv4 сервера для `announce`.
- Домен для `tls_domain`.
- Для Mini App (опционально): публичный HTTPS-домен и reverse proxy (например, Caddy) до `127.0.0.1:8090`.

## Быстрый старт (Linux)

Основной сценарий установки теперь - один bootstrap-скрипт, который:

- скачивает последние релизы `telemt` и `telemt-admin`;
- кладёт бинарники в `/usr/local/bin`;
- создаёт `telemt.toml`, `telemt-admin.toml`, `systemd` unit-файлы и Polkit-правило;
- включает и запускает `telemt.service` и `telemt-admin.service`.

Запуск из интерактивного терминала:

```bash
curl -fsSL https://raw.githubusercontent.com/fgbm/telemt-admin/master/scripts/install.sh | sudo bash
```

Установщик задаёт интерактивные вопросы и читает ответы из терминала. Если TTY недоступен, скрипт завершится с ошибкой вместо бесконечных повторных prompt'ов.

Для CI, automation или сессий без TTY сначала сохраните скрипт в файл и запустите его локально:

```bash
curl -fsSLo install.sh https://raw.githubusercontent.com/fgbm/telemt-admin/master/scripts/install.sh
sudo bash ./install.sh
```

Во время установки скрипт спросит только минимально необходимые значения:

- `bot_token`
- `admin_ids`
- `port` для `telemt` (по умолчанию `443`)
- `tls_domain`
- `announce` (публичный IPv4 сервера)

После завершения можно проверить статус:

```bash
sudo systemctl status telemt.service
sudo systemctl status telemt-admin.service
```

Скрипт генерирует минимальный `/etc/telemt.toml` такого вида:

```toml
[general]
use_middle_proxy = false

[general.modes]
classic = false
secure = false
tls = true

[server]
port = 443

[server.api]
enabled = true
listen = "127.0.0.1:9091"
whitelist = ["127.0.0.1/32", "::1/128"]
auth_header = "Bearer <generated-secret>"

[censorship]
tls_domain = "site.example"

[[server.listeners]]
ip = "0.0.0.0"
announce = "X.X.X.X"
```

## Быстрый старт (Docker)

Если `telemt-admin` должен работать в контейнере, а сам `telemt` уже доступен по сети, подготовьте отдельную директорию для compose-стека:

```bash
mkdir -p telemt-admin-docker/config telemt-admin-docker/data
cd telemt-admin-docker
curl -fsSLo docker-compose.yml https://raw.githubusercontent.com/fgbm/telemt-admin/master/deploy/compose/docker-compose.telemt-admin.example.yml
curl -fsSLo .env.example https://raw.githubusercontent.com/fgbm/telemt-admin/master/deploy/compose/.env.example
curl -fsSLo config/telemt-admin.toml https://raw.githubusercontent.com/fgbm/telemt-admin/master/deploy/docker/telemt-admin.docker.toml.example
cp .env.example .env
```

Дальше:

1. Укажите в `config/telemt-admin.toml` как минимум `bot_token` и `admin_ids`, при необходимости — `telemt_api.auth_header` для control API. При необходимости задайте `TELEMT_ADMIN_VERSION` в `.env` (по умолчанию `latest`). Если из контейнера недоступен `api.telegram.org`, добавьте в `.env` `TELEMT_ADMIN__BOT_USERNAME` (username без `@`), иначе ссылки на токены и deep link не соберутся.
2. Отредактируйте при необходимости `telemt_api.base_url` так, чтобы URL был достижим из сети контейнера.
3. Режим `external` и метка для UI задаются в `docker-compose.yml` через `TELEMT_ADMIN__RUNTIME__MODE` / `TELEMT_ADMIN__RUNTIME__LABEL`. Держите `allow_file_fallback = false`, если не монтируете `telemt.toml`.
4. Если хотите конфигурируемые тексты бота, добавьте секцию `[bot_messages]` в TOML или переменные `TELEMT_ADMIN__BOT_MESSAGES__*` в `.env`.
5. Пример Compose ориентирован на API-only path. Если нужен legacy fallback с записью в `telemt.toml`, потребуется отдельный RW mount/volume, которого в примере нет.
6. Запустите стек:

```bash
docker compose pull
docker compose up -d
docker compose logs -f telemt-admin
```

Минимальный профиль `config/telemt-admin.toml` для Docker:

```toml
bot_token = "ВАШ_ТОКЕН_БОТА"
admin_ids = [123456789]

db_path = "/var/lib/telemt-admin/state.db"
telemt_config_path = "/etc/telemt.toml"

[telemt_api]
enabled = true
base_url = "http://telemt:9091"
timeout_ms = 5000
allow_file_fallback = false
prefer_api_links = true

[bot_messages]
start_without_invite = "Привет. Бот работает в закрытом режиме."
user_link_template = "Ваша ссылка:\n\n{link}"
access_approved_template = "Доступ готов.\n\n{link}"
```

Если `telemt` живёт в другом compose-стеке или на хосте, используйте такой `telemt_api.base_url`, который реально достижим из сети контейнера `telemt-admin`.

## Ручная установка

Если хотите развернуть всё вручную без bootstrap-скрипта, ниже остаётся reference-сценарий.

### 1. Скачивание актуальной версии `telemt-admin`

```bash
curl -L -o telemt-admin.tar.gz https://github.com/fgbm/telemt-admin/releases/latest/download/telemt-admin-linux-x86_64.tar.gz && tar -xzf telemt-admin.tar.gz
```

### 2. Установка бинарного файла

```bash
sudo mv telemt-admin /usr/local/bin/
sudo chmod +x /usr/local/bin/telemt-admin
```

### 3. Минимальная конфигурация `telemt-admin`

Создайте файл `/etc/telemt-admin.toml`:

```toml
bot_token = "ВАШ_ТОКЕН_БОТА"
admin_ids = [123456789] # Ваш Telegram ID
telemt_config_path = "/etc/telemt.toml"
db_path = "/var/lib/telemt-admin/state.db"
service_name = "telemt.service"
users_page_size = 10

[security]
default_token_days = 14
max_token_days = 180
allow_auto_approve_tokens = true
wizard_state_ttl_seconds = 86400 # опционально: TTL wizard-state в секундах

[telemt_api]
enabled = true
base_url = "http://127.0.0.1:9091"
auth_header = "Bearer ВАШ_ТОЧНЫЙ_AUTH_HEADER"
timeout_ms = 5000
allow_file_fallback = true
prefer_api_links = true

[notifications]
enabled = true
health_check_interval_secs = 60
notify_on_health_change = true
notify_on_runtime_alerts = true
notify_on_new_request = true
```

> [!TIP]
> Параметр `bot_token` можно не указывать в конфиге, если переменная окружения `TELOXIDE_TOKEN` задана в окружении процесса.

## Установка как системного сервиса

Для надежной работы в фоновом режиме настройте `systemd`:

### 1. Создайте пользователя и директории

```bash
sudo useradd --system --home /var/lib/telemt-admin --shell /usr/sbin/nologin telemt-admin
sudo mkdir -p /var/lib/telemt-admin
sudo chown -R telemt-admin:telemt-admin /var/lib/telemt-admin
```

### 2. Создайте unit-файл `/etc/systemd/system/telemt-admin.service`

```ini
[Unit]
Description=telemt-admin Telegram Bot Service
After=network-online.target

[Service]
Type=simple
User=telemt-admin
Group=telemt-admin
WorkingDirectory=/var/lib/telemt-admin
ExecStart=/usr/local/bin/telemt-admin /etc/telemt-admin.toml
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

### 3. Настройте права доступа

Чтобы сервис мог управлять `telemt` и редактировать его конфиг без прав root:

#### А. Разрешите управление `telemt.service` через Polkit

Создайте файл `/etc/polkit-1/rules.d/50-telemt-admin.rules`:

```javascript
polkit.addRule(function(action, subject) {
    if (action.id == "org.freedesktop.systemd1.manage-units" &&
        action.lookup("unit") == "telemt.service" &&
        subject.user == "telemt-admin") {
        return polkit.Result.YES;
    }
});
```

Этого правила достаточно для команд `systemctl restart` и
`systemctl kill -s HUP --kill-who=main telemt.service`, которые использует бот.

#### Б. Настройте права на конфиг telemt

```bash
# Создаем группу telemt если её нет
sudo groupadd -f telemt

# Добавляем пользователя бота в группу
sudo usermod -aG telemt telemt-admin

# Меняем группу владельца конфига и даем права на запись группе
sudo chown :telemt /etc/telemt.toml
sudo chmod 664 /etc/telemt.toml
```

При такой настройке у пользователя `telemt-admin` есть право **читать fallback-конфиг** и при необходимости использовать legacy-путь записи в `/etc/telemt.toml`. В штатном режиме новые установки используют control API `telemt`, поэтому прямое редактирование файла больше не является основным механизмом.

### 4. Запустите сервис

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now telemt-admin.service
```

## Проверка после запуска

Проверьте, что сервис запустился и бот отвечает:

```bash
sudo systemctl status telemt-admin.service
sudo journalctl -u telemt-admin.service -n 50 --no-pager
```

Минимальный smoke-check:

1. Напишите боту `/start` с аккаунта администратора.
2. Убедитесь, что открывается главный inline-экран администратора.
3. Откройте `/token`, создайте токен через кнопки и активируйте его пользовательским аккаунтом.
4. Откройте `/user`, зайдите в карточку пользователя и убедитесь, что видны live-данные и кнопки изменения лимитов.
5. Откройте `/service` и убедитесь, что видны runtime snapshot, блок нагрузки и экран `Top пользователей`.
6. Если включён `[webapp]`: `curl -fsS http://127.0.0.1:8090/healthz` отвечает, в логе есть `Mini App: кнопка меню Mini App установлена`, приложение открывается кнопкой меню у пользователя и у администратора.

## Обновление

### Обновление telemt (при установке быстрым скриптом)

```bash
curl -fsSL https://github.com/telemt/telemt/releases/latest/download/telemt-$(uname -m)-linux-gnu.tar.gz | sudo tar -xz -C /usr/local/bin/ && sudo systemctl restart telemt.service
```

### Проверка и автообновление telemt-admin

Проверить наличие новой версии:

```bash
telemt-admin check-update
```

На Linux x86_64 доступно автообновление (требуются права на запись в директорию с бинарником):

```bash
sudo telemt-admin self-update
```

Если бинарник установлен в `/usr/local/bin/` и сервис запущен от пользователя `telemt-admin`, для автообновления потребуется выполнить команду от root. После успешного обновления перезапустите сервис:

```bash
sudo systemctl restart telemt-admin.service
```

На Windows и других платформах автообновление не поддерживается — используйте ручное скачивание из раздела «Быстрый старт».

### Обновление Docker-развёртывания

Если `telemt-admin` запущен через Docker Compose, обновление сводится к подтягиванию нового тега образа и перезапуску контейнера:

```bash
cd telemt-admin-docker
docker compose pull
docker compose up -d
docker compose logs --tail=100 telemt-admin
```

Если вы хотите обновляться не на `latest`, а на фиксированную версию, задайте в `.env`, например, `TELEMT_ADMIN_VERSION=0.1.15`, и затем снова выполните `docker compose pull && docker compose up -d`.

## Docker

В репозитории есть `Dockerfile` (образ только с бинарником, **без** systemd) и пример Compose: [`deploy/compose/docker-compose.telemt-admin.example.yml`](../deploy/compose/docker-compose.telemt-admin.example.yml). Для Docker quick start также есть шаблон `.env`: [`deploy/compose/.env.example`](../deploy/compose/.env.example). Кратко: [`deploy/compose/README.md`](../deploy/compose/README.md).

При создании GitHub-тега версии `vX.Y.Z` workflow релиза публикует образ в `ghcr.io/<owner>/telemt-admin` с тегами `X.Y.Z`, `X.Y`, `X`, `sha-<commit>` и `latest`.

В образе поставляется пример конфигурации без секретов: **`/usr/share/doc/telemt-admin/docker-default.toml.example`** (исходник: [`deploy/docker/telemt-admin.docker.toml.example`](../deploy/docker/telemt-admin.docker.toml.example)). Его можно сохранить как `config/telemt-admin.toml` в compose-директории или взять за основу иным способом. Docker-образ по умолчанию ожидает рабочий конфиг по пути **`/etc/telemt-admin/telemt-admin.toml`** и объявляет тома **`/etc/telemt-admin`** и **`/var/lib/telemt-admin`**. Процесс в контейнере запускается от **root**, чтобы не ловить рассинхрон uid/gid на смонтированном каталоге данных; граница безопасности — изоляция контейнера и политика хоста.

Рекомендуемый профиль для контейнера:

- режим `external` (в примере Compose задаётся через `TELEMT_ADMIN__RUNTIME__MODE`);
- `[telemt_api] enabled = true` и `base_url` на доступный с сети контейнера control API telemt;
- `allow_file_fallback = false`, если нет смонтированного `telemt.toml` и systemd;
- bind mount или том на `db_path` для SQLite;
- при необходимости — переопределения через `TELEMT_ADMIN__*` (см. [configuration.md](configuration.md#переменные-окружения-telemt_admin__-overlay)).

## Сборка из исходников

Если вы хотите собрать бота самостоятельно:

1. Установите Rust (Edition 2024).
2. Выполните сборку: `cargo build --release`.
3. Бинарный файл будет находиться в `target/release/telemt-admin`.

### CLI

- `telemt-admin --help` / `-h` — справка по аргументам.
- `telemt-admin --version` / `-V` — версия.
- `telemt-admin -c <FILE>` / `--config <FILE>` — путь к конфигу.
- `telemt-admin check-update` — проверить наличие новой версии.
- `telemt-admin self-update` — автообновление (только Linux x86_64).

Запуск с кастомным конфигом (оба варианта поддерживаются):

```bash
./target/release/telemt-admin /path/to/telemt-admin.toml
./target/release/telemt-admin --config /path/to/telemt-admin.toml
```
