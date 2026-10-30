# Конфигурация (telemt-admin.toml)

- `bot_token` — токен бота от @BotFather (опционально, если задан `TELEMT_ADMIN__BOT_TOKEN` или `TELOXIDE_TOKEN`).
- `bot_username` — username бота в Telegram **без** ведущего `@` (опционально). Нужен для ссылок на invite-токены и deep link, если до `api.telegram.org` нет исходящего доступа и запрос `getMe` не проходит; при указании имеет приоритет над ответом `getMe`.
- `admin_ids` — массив ID администраторов `[123, 456]` (обязательный).
- `telemt_config_path` — путь к `/etc/telemt.toml` (default: `/etc/telemt.toml`).
- `db_path` — путь к `state.db` (default: `/var/lib/telemt-admin/state.db`).
- `service_name` — имя systemd unit для telemt (default: `telemt.service`). Используется в режиме `runtime.mode = "systemd"` и как запасной вариант, если в `[runtime]` не задано своё `service_name`.
- `[runtime]` — как бот видит управление процессом telemt на машине (опционально; если секции нет, считается `mode = "systemd"` и unit из `service_name`):
  - `mode` — `systemd` | `external` | `none` (default при явной секции: `systemd`).
  - `service_name` — override имени unit для `systemd` (иначе — верхнеуровневый `service_name`).
  - `label` — подпись в UI для `external` (например `docker compose`).
- `users_page_size` — размер страницы для списков в админском интерфейсе: пользователей, invite-токенов и заявок (default: `10`).
- `[telemt_api]` — настройки control API `telemt`:
  - `enabled` — включить API-first backend.
  - `base_url` — базовый URL control API, например `http://127.0.0.1:9091`.
  - `auth_header` — точное значение заголовка `Authorization`, которое ожидает `telemt`.
  - `timeout_ms` — таймаут HTTP-клиента.
  - `allow_file_fallback` — разрешить fallback на legacy-конфиг и `systemd`, если API недоступен.
  - `prefer_api_links` — приоритетно использовать ссылки, которые вернул control API.
- `[web_proxy]` — WEB-прокси `telemt` (тип прокси WEB в Telegram Desktop 7.1+, `telemt` 3.5+):
  - `host` — публичный hostname WEB-прокси без схемы, порта и пути, например `lk.example.com`. Если задан, бот добавляет к каждому сообщению со ссылкой блок с `tg://webproxy?server=<host>&secret=<dd>…`; секрет берётся из обычной ссылки пользователя. Пусто — WEB-ссылки не выдаются.
  - `secret_mode` — `dd` (по умолчанию) или `plain`; должен совпадать с `secret_mode` профиля в `[[web.vhosts.profiles]]` `telemt`.
  - `base_path` — `[[web.vhosts]].base_path` vhost `telemt`, например `chill` или `telegram/web` (без крайних `/`). Если задан, бот выдаёт path-ссылку в формате `telemt`: `server=<host>%2F<base_path>`, секрет — base64url от `0x70 || [0xdd] || secret`. Пусто — root-ссылка с hex-секретом. Некорректное значение отключает WEB-ссылки (предупреждение в логе при старте).
  - `link_scheme` — вид ссылки: `tg` (по умолчанию, `tg://webproxy?…`) или `https` (`https://t.me/webproxy?…`).
  - `manage_profiles` — `true`: при одобрении бот сам добавляет пользователю профиль в `[[web.vhosts.profiles]]` через control API (`PATCH /v1/config`) и сразу делает reload `telemt` (по умолчанию `false`; нужен `telemt_api.enabled`). Подробности — [ADR 006](adr/006-web-profiles-management.md).
  - `reload_drain_secs` — сколько секунд старые сессии `telemt` доживают при таком reload (drain), от 1 до 3600 (по умолчанию 300); затем они закрываются и клиенты переподключаются.
  - Перед удалением пользователя бот всегда убирает его WEB-профили: иначе `telemt` отклоняет удаление.
  - Без `manage_profiles` бот **не** создаёт WEB-профили в `telemt`: без профиля у пользователя WEB-ссылка не заработает.
- `[webapp]` — встроенное веб-приложение Telegram Mini App (ADR 005):
  - `enabled` — запустить HTTP-сервер приложения (default: `false`).
  - `listen` — адрес сервера (default: `127.0.0.1:8090`); наружу публикуется только через HTTPS reverse proxy.
  - `public_url` — публичный HTTPS URL, например `https://app.example.com/`; при заданном URL бот ставит кнопку меню, открывающую приложение.
  - `init_data_max_age_secs` — максимальный возраст подписанного `initData`, от 60 до 86400 (default: `3600`); значение вне диапазона — ошибка конфигурации.
  - `menu_button_text` — текст кнопки меню (default: `Прокси`).
  - Пользователь видит статус доступа, обычную и WEB-ссылку (копирование, QR, «Подключить»), инструкцию и может отправить invite-токен боту. Администратор — заявки, пользователей (поиск, карточка, удаление), invite-токены (создание, отзыв) и статус `telemt`.
  - Пример reverse proxy (Caddy): `app.example.com { reverse_proxy 127.0.0.1:8090 }`.
- `[security]` — настройки безопасности invite-токенов (срок действия **ссылки** и лимит активаций; не путать с лимитами пользователя в `telemt`):
  - `default_token_days` — срок действия invite-ссылки по умолчанию, в днях (default: 14).
  - `max_token_days` — максимально допустимый срок действия invite-ссылки (default: 180).
  - `allow_auto_approve_tokens` — разрешить создание auto-approve токенов (default: `true`).
  - `wizard_state_ttl_seconds` — опциональный TTL для сохранённого wizard-state в секундах. Если не задан, состояние хранится, пока сценарий не завершат или не отменят.
- `[notifications]` — фоновые уведомления и мониторинг:
  - `enabled` — включить фоновый polling `telemt` API и уведомления.
  - `health_check_interval_secs` — интервал health-check и runtime polling в секундах.
  - `notify_on_health_change` — уведомлять о смене health/API availability/accepting state/ME readiness.
  - `notify_on_runtime_alerts` — уведомлять о `unhealthy upstream`, ошибках `ME self-test` и восстановлении после них.
  - `notify_on_new_request` — отправлять уведомления о новых manual-заявках.
- `[bot_messages]` — переопределяемые тексты пользовательского UX и рассылки:
  - `start_without_invite` — текст-заглушка для `/start` без invite-токена; если задан, бот не переводит пользователя сразу в wizard ввода токена.
  - `invite_manual_prompt` — текст первого запроса invite-токена.
  - `invite_followup_prompt` — текст после кнопки «Ввести invite-токен».
  - `user_link_template` — шаблон сообщения со ссылкой; поддерживает `{link}`.
  - `access_approved_template` — шаблон auto-approve; поддерживает `{link}`.
  - `web_link_template` — блок с WEB-ссылкой, который добавляется к сообщениям со ссылкой при заданном `[web_proxy] host`; поддерживает `{web_link}`.
  - `request_submitted` — сообщение после создания manual-заявки.
  - `request_pending` — сообщение, если заявка уже ожидает решения.
  - `request_rejected` — сообщение для отклонённой заявки.
  - `broadcast_prompt` — подсказка перед рассылкой; поддерживает `{audience}`.
  - `broadcast_cancelled` — сообщение при отмене рассылки пустым текстом.
  - `broadcast_summary_template` — итог рассылки; поддерживает `{ok}`, `{failed}`, `{total}`.
  - `feedback_prompt` — подсказка после кнопки «Обратная связь».
  - `feedback_sent` — подтверждение пользователю, что сообщение передано администратору.
  - `feedback_reply_header` — заголовок перед ответом администратора.
  - `admin_feedback_template` — заголовок сообщения обратной связи у админов; поддерживает `{user_id}`, `{username}`, `{display_name}`.
  - `donation_text` — текст по кнопке «💝 Пожертвование» (реквизиты, ссылки). Пусто — кнопка не показывается. Для многострочного текста используйте `"""…"""`; после изменения перезапустите бота.

## Переменные окружения `TELEMT_ADMIN__*` (overlay)

После чтения TOML можно переопределить отдельные поля через **whitelist** переменных с префиксом `TELEMT_ADMIN__` (приоритет выше, чем значения из файла). Полный список и правила — в [`docs/adr/004-config-sources-and-docker-defaults.md`](adr/004-config-sources-and-docker-defaults.md).

Примеры:

- `TELEMT_ADMIN__BOT_TOKEN` — токен бота (альтернатива полю в TOML; также поддерживается `TELOXIDE_TOKEN`, если `bot_token` не задан).
- `TELEMT_ADMIN__BOT_USERNAME` — username без `@` (альтернатива полю `bot_username` в TOML).
- `TELEMT_ADMIN__ADMIN_IDS` — список ID через запятую, например `123,456`.
- `TELEMT_ADMIN__TELEMT_API__BASE_URL` — URL control API.
- `TELEMT_ADMIN__RUNTIME__MODE` — `systemd`, `external` или `none`.
- `TELEMT_ADMIN__BOT_MESSAGES__START_WITHOUT_INVITE` — заглушка для `/start` без invite.
- `TELEMT_ADMIN__BOT_MESSAGES__USER_LINK_TEMPLATE` — шаблон сообщения со ссылкой.
- `TELEMT_ADMIN__BOT_MESSAGES__BROADCAST_SUMMARY_TEMPLATE` — шаблон итоговой сводки рассылки.

Не вшивайте секреты в `Dockerfile` через `ENV`: передавайте их через Compose `environment`, `env_file` или секреты оркестратора.

## Wizard-state и БД

- Пошаговое состояние wizard хранится в SQLite-таблице `bot_wizard_states`.
- Схема БД обновляется автоматически при старте бота.
- Источник истины для wizard-state — SQLite, поэтому активный сценарий переживает рестарт процесса.
- Если задан `security.wizard_state_ttl_seconds`, wizard-state старше TTL удаляется автоматически и не восстанавливается.
