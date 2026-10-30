# Установка и обновление

Требования, установка (скриптом, в Docker, вручную), проверка после запуска, обновление и сборка из исходников. Параметры конфига описаны в [configuration.md](configuration.md).

## Требования

- Linux-сервер с `systemd`.
- `root`-доступ или `sudo`.
- Telegram-бот и токен от [@BotFather](https://t.me/BotFather).
- Telegram user ID администраторов (можно получить через `@userinfobot`).
- Публичный IPv4 сервера для `announce`.
- Домен для `tls_domain`.
- Для Mini App (опционально): публичный HTTPS-домен и reverse proxy (например, Caddy) до `127.0.0.1:8090`.

## Быстрый старт (Linux)

Основной способ установки — bootstrap-скрипт. Он:

- скачивает последние релизы `telemt` и `telemt-admin`;
- кладёт бинарники в `/usr/local/bin`;
- создаёт `telemt.toml`, `telemt-admin.toml`, `systemd` unit-файлы и Polkit-правило;
- включает и запускает `telemt.service` и `telemt-admin.service`.

Запуск из интерактивного терминала:

```bash
curl -fsSL https://raw.githubusercontent.com/fgbm/telemt-admin/master/scripts/install.sh | sudo bash
```

Установщик задаёт вопросы и читает ответы из терминала. Если TTY нет, скрипт завершится с ошибкой, а не будет повторять вопросы бесконечно.

Для CI, автоматизации и сессий без TTY сохраните скрипт в файл и запустите его локально:

```bash
curl -fsSLo install.sh https://raw.githubusercontent.com/fgbm/telemt-admin/master/scripts/install.sh
sudo bash ./install.sh
```

Скрипт спросит только самое необходимое:

- `bot_token`
- `admin_ids`
- `port` для `telemt` (по умолчанию `443`)
- `tls_domain`
- `announce` (публичный IPv4 сервера)

После установки проверьте статус:

```bash
sudo systemctl status telemt.service
sudo systemctl status telemt-admin.service
```

Скрипт создаёт минимальный `/etc/telemt.toml` такого вида:

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

Если `telemt-admin` будет работать в контейнере, а `telemt` уже доступен по сети, подготовьте для compose-стека отдельную директорию:

```bash
mkdir -p telemt-admin-docker/config telemt-admin-docker/data
cd telemt-admin-docker
curl -fsSLo docker-compose.yml https://raw.githubusercontent.com/fgbm/telemt-admin/master/deploy/compose/docker-compose.telemt-admin.example.yml
curl -fsSLo .env.example https://raw.githubusercontent.com/fgbm/telemt-admin/master/deploy/compose/.env.example
curl -fsSLo config/telemt-admin.toml https://raw.githubusercontent.com/fgbm/telemt-admin/master/deploy/docker/telemt-admin.docker.toml.example
cp .env.example .env
```

Затем:

1. Укажите в `config/telemt-admin.toml` как минимум `bot_token` и `admin_ids`, а если нужно, ещё `telemt_api.auth_header` для control API. Версию образа можно задать в `.env` через `TELEMT_ADMIN_VERSION` (по умолчанию `latest`). Если из контейнера недоступен `api.telegram.org`, добавьте в `.env` `TELEMT_ADMIN__BOT_USERNAME` (username без `@`), иначе бот не соберёт ссылки на токены и deep link.
2. Если нужно, поправьте `telemt_api.base_url`: URL должен быть достижим из сети контейнера.
3. Режим `external` и метка для UI уже заданы в `docker-compose.yml` через `TELEMT_ADMIN__RUNTIME__MODE` / `TELEMT_ADMIN__RUNTIME__LABEL`. Если не монтируете `telemt.toml`, оставьте `allow_file_fallback = false`.
4. Свои тексты бота задаются секцией `[bot_messages]` в TOML или переменными `TELEMT_ADMIN__BOT_MESSAGES__*` в `.env`.
5. Пример Compose рассчитан только на работу через API. Для legacy fallback с записью в `telemt.toml` нужен отдельный RW mount или volume, в примере его нет.
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

Если `telemt` запущен в другом compose-стеке или на хосте, укажите в `telemt_api.base_url` адрес, до которого действительно можно достучаться из сети контейнера `telemt-admin`.

## Ручная установка

Ниже шаги для установки вручную, без bootstrap-скрипта.

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

Чтобы бот надежно работал в фоне, настройте `systemd`:

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

С такими правами пользователь `telemt-admin` может **читать fallback-конфиг** и при необходимости писать в `/etc/telemt.toml` по legacy-пути. Новые установки в штатном режиме работают через control API `telemt`, так что прямая правка файла теперь не основной механизм.

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

Минимальная проверка:

1. Напишите боту `/start` с аккаунта администратора.
2. Должен открыться главный inline-экран администратора.
3. Откройте `/token`, создайте токен кнопками и активируйте его с аккаунта пользователя.
4. Откройте `/user` и карточку пользователя: в ней должны быть live-данные и кнопки изменения лимитов.
5. Откройте `/service`: там должны быть runtime snapshot, блок нагрузки и экран `Top пользователей`.
6. Если включён `[webapp]`, проверьте, что `curl -fsS http://127.0.0.1:8090/healthz` отвечает, в логе есть `Mini App: кнопка меню Mini App установлена`, а приложение открывается кнопкой меню и у пользователя, и у администратора.

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

`self-update` скачивает последний релиз из upstream (`fgbm/telemt-admin`). Если у вас сборка этого форка, автообновление заменит её версией без функций форка: обновляйтесь пересборкой из исходников.

Если бинарник лежит в `/usr/local/bin/`, а сервис запущен от пользователя `telemt-admin`, команду автообновления нужно выполнять от root. После обновления перезапустите сервис:

```bash
sudo systemctl restart telemt-admin.service
```

На Windows и других платформах автообновления нет: скачайте новую версию вручную, как в разделе «Ручная установка».

### Обновление Docker-развёртывания

Если `telemt-admin` запущен через Docker Compose, подтяните новый тег образа и перезапустите контейнер:

```bash
cd telemt-admin-docker
docker compose pull
docker compose up -d
docker compose logs --tail=100 telemt-admin
```

Чтобы обновляться не на `latest`, а на конкретную версию, задайте её в `.env`, например `TELEMT_ADMIN_VERSION=0.1.15`, и снова выполните `docker compose pull && docker compose up -d`.

## Docker

В репозитории есть `Dockerfile` (образ только с бинарником, **без** systemd) и пример Compose: [`deploy/compose/docker-compose.telemt-admin.example.yml`](../deploy/compose/docker-compose.telemt-admin.example.yml). Для быстрого старта в Docker есть шаблон `.env`: [`deploy/compose/.env.example`](../deploy/compose/.env.example). Кратко: [`deploy/compose/README.md`](../deploy/compose/README.md).

Когда на GitHub создаётся тег версии `vX.Y.Z`, release workflow публикует образ в `ghcr.io/<owner>/telemt-admin` с тегами `X.Y.Z`, `X.Y`, `X`, `sha-<commit>` и `latest`.

В образе поставляется пример конфигурации без секретов: **`/usr/share/doc/telemt-admin/docker-default.toml.example`** (исходник: [`deploy/docker/telemt-admin.docker.toml.example`](../deploy/docker/telemt-admin.docker.toml.example)). Его можно сохранить как `config/telemt-admin.toml` в compose-директории или взять за основу своего конфига. Docker-образ по умолчанию ожидает рабочий конфиг по пути **`/etc/telemt-admin/telemt-admin.toml`** и объявляет тома **`/etc/telemt-admin`** и **`/var/lib/telemt-admin`**. Процесс в контейнере работает от **root**, чтобы не было рассинхрона uid/gid на смонтированном каталоге данных. Безопасность в этом случае обеспечивают изоляция контейнера и политика хоста.

Рекомендуемый профиль для контейнера:

- режим `external` (в примере Compose задаётся через `TELEMT_ADMIN__RUNTIME__MODE`);
- `[telemt_api] enabled = true` и `base_url` на доступный с сети контейнера control API telemt;
- `allow_file_fallback = false`, если нет смонтированного `telemt.toml` и systemd;
- bind mount или том на `db_path` для SQLite;
- при необходимости — переопределения через `TELEMT_ADMIN__*` (см. [configuration.md](configuration.md#переменные-окружения-telemt_admin__-overlay)).

## Сборка из исходников

Чтобы собрать бота самостоятельно:

1. Установите Rust (Edition 2024).
2. Выполните сборку: `cargo build --release`.
3. Бинарник появится в `target/release/telemt-admin`.

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
