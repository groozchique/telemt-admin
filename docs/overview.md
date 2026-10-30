# Обзор проекта

`telemt-admin` — Telegram-бот для администрирования доступа к `telemt` (MTProto proxy).

Текущая целевая структура:

- Telegram handlers и экраны отвечают за UX и навигацию;
- SQLite-слой разделяется на узкие подмодули репозиториев в `src/db/*.rs`;
- интеграция с `telemt` проходит через единый backend-слой с API-first стратегией и fallback на `telemt.toml`/`systemd`;
- фоновый мониторинг и push-уведомления администраторам выполняются отдельно от Telegram handlers;
- тексты интерфейса не должны собираться внутри SQL-запросов;
- пользовательские тексты бота можно задавать в `[bot_messages]` и через `TELEMT_ADMIN__BOT_MESSAGES__*`.

Основные сценарии:

- регистрация и выдача нового доступа пользователю по invite-токену;
- ручное или автоматическое одобрение;
- выдача proxy-ссылки и QR;
- управление существующими пользователями (поиск, карточка, удаление) и их лимитами;
- управление invite-токенами;
- управление `systemd`-сервисом `telemt` и наблюдение за control API;
- live runtime-диагностика, sync-health и top users по соединениям/трафику;
- фоновые уведомления о health/runtime проблемах;
- уведомления пользователю об удалении доступа, о скором окончании и истечении срока и о расходе квоты трафика; показ срока и трафика в `/link` и Mini App;
- рассылка сообщений всем пользователям со статусом approved (админ-панель);
- обратная связь: пользователь пишет администраторам через бота (текст или медиа), администратор отвечает кнопкой «Ответить»;
- группы пользователей: назначение из карточки, массовое отключение, задание/очистка общего срока действия и применение этого срока к участникам через PATCH;
- импорт в локальную БД пользователя, уже существующего в telemt API, по числовому Telegram user id (при `telemt_api.enabled`).
- автоматический подхват существующего пользователя `tg_<id>` из `telemt API` в user flow (`/start`, `/link`, invite) без отдельного ручного импорта;

Технологический стек:

- Rust edition `2024`
- `teloxide`
- `SQLite` через `sqlx`
- `toml` и `toml_edit`
- `reqwest` для `telemt` control API
- `tracing`, `tracing-subscriber`

Целевой production-deploy для текущего MVP:

- Docker-образ для Linux x86_64 (glibc);
- типовой запуск через Docker Compose / иной container runtime в режиме `[runtime] = external`;
- bootstrap-установка через `scripts/install.sh` и хостовый `systemd` остаётся поддерживаемым fallback-сценарием.

Контейнерный production-сценарий опирается на секцию `[runtime]`, режим `external` и control API; см. `docs/adr/003-runtime-agnostic-deployment.md`. Для Docker допустимы точечные overrides через whitelist `TELEMT_ADMIN__*` (см. `docs/adr/004-config-sources-and-docker-defaults.md`). Если до Telegram Bot API нет исходящего доступа, username для ссылок на токены задаётся явно (`bot_username` в TOML или `TELEMT_ADMIN__BOT_USERNAME`).

Ключевые инварианты:

- пользователь `telemt` маппится как `tg_<telegram_user_id>`;
- при включённом control API операции CRUD над пользователями должны идти через API; legacy-запись в `telemt.toml` используется как fallback;
- `/service` и monitoring-path должны честно показывать частичную деградацию control API по секциям, а не маскировать её общим «нет данных»;
- `telemt` поддерживает hot reload конфига, поэтому legacy-путь всё ещё может использовать `HUP/restart`, но это больше не основной механизм синхронизации;
- invite-токены учитывают срок действия, `is_active` и `max_usage`; для уже одобренного пользователя повторный переход по ссылке не увеличивает `usage_count`;
- если локальной approved-записи ещё нет, но пользователь уже существует в `telemt API` как `tg_<id>`, user flow может автоматически импортировать его в локальную БД без расходования invite usage;
- в заявке может сохраняться `invite_token_id` (источник invite);
- критичные переходы состояний регистрации и consume invite-токена должны оставаться атомарными и не затирать конкурентные изменения;
- wizard-state хранится в SQLite, переживает рестарт процесса и может иметь TTL через `security.wizard_state_ttl_seconds`;
- если администратор запрашивает `/link` без существующей учётной записи, доступ создаётся автоматически;
- `/start` может использоваться не только для invite-токена, но и для admin deep links на карточки и экраны;
- уведомления и polling-поведение определяются секцией `[notifications]` в `telemt-admin.toml`.
- блокирующий файловый I/O (`telemt.toml`, self-update unpack/copy/rename) должен быть вынесен из async executor через явный offload.

Документация решений:

- индекс ADR: `docs/adr/README.md`;
- архитектурные решения по backend-слою: `docs/adr/001-telemt-api-backend.md`;
- решения по безопасности и rollout: `docs/adr/002-telemt-api-security-and-rollout.md`;
- операционный runbook: `docs/runbook.md`.
- backlog дальнейшей реализации: `docs/backlog.md`.
