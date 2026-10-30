<p align="center">
  <img src="logo.png" alt="telemt-admin logo" width="800">
</p>

<h1 align="center">telemt-admin</h1>

**telemt-admin** — Telegram-бот, который управляет пользователями [telemt](https://github.com/telemt/telemt) (MTProto прокси-сервера). Он нужен системному администратору, который хочет отдать выдачу доступа боту и не править конфиги руками.

Доступ выдаётся по пригласительным токенам. Токен работает в одном из двух режимов:

- ручной (Manual): пользователь подаёт заявку, админ её подтверждает;
- автоматический (Auto-approve): пользователь сразу получает доступ, админ не участвует.

У invite-токена при создании задаются срок действия ссылки (в днях) и лимит активаций, то есть сколько раз по ссылке можно зарегистрироваться. Это **не** срок подписки, и после регистрации токен **не** ограничивает доступ пользователя в `telemt`. Дату истечения доступа, квоты трафика и прочие лимиты настраивают отдельно в карточке пользователя в боте (через control API `telemt`).

## Что добавляет этот форк

Это форк [fgbm/telemt-admin](https://github.com/fgbm/telemt-admin). Всё, что есть в upstream, работает так же, а сверху добавлено следующее.

- WEB-прокси с `base_path`. Если WEB-прокси стоит не в корне vhost, задайте `[web_proxy] base_path`: бот выдаст path-ссылку в формате `telemt` (`server=<host>%2F<base_path>`, секрет в base64url).
- Вид WEB-ссылки: `[web_proxy] link_scheme = "tg" | "https"` выбирает между `tg://webproxy?…` и `https://t.me/webproxy?…`.
- Автоматические WEB-профили. С `[web_proxy] manage_profiles = true` бот при одобрении сам добавляет пользователю профиль в `[[web.vhosts.profiles]]` через control API и делает reload `telemt` с drain (`reload_drain_secs`). При запуске бот добавляет профили пользователям, у которых их нет. Перед удалением пользователя профили снимаются всегда, иначе `telemt` отклоняет удаление; если удаление не прошло, профиль возвращается. Подробности в [ADR 006](docs/adr/006-web-profiles-management.md).
- Обратная связь. На главном экране пользователя в боте есть кнопка «✉️ Обратная связь» (в Mini App её нет). Сообщение пользователя (текст, медиа, альбом) копируется всем `admin_ids` с кнопкой «↩️ Ответить», ответ приходит пользователю от имени бота. Тексты задаются в `[bot_messages]`: `feedback_*` и `admin_feedback_template`.
- Пожертвования: кнопка «💝 Пожертвование» на главном экране пользователя в боте показывает текст из `[bot_messages] donation_text`. Если текст не задан, кнопки нет.

## Возможности

- Пользователь регистрируется в боте по пригласительному токену и получает ссылку с QR, а также WEB-ссылку `tg://webproxy?…` для Telegram Desktop 7.1+ (`telemt` 3.5+).
- Администратору доступны заявки, invite-токены, карточки пользователей с live-данными и лимитами, группы, рассылка, импорт из `telemt`, панель `/service` и фоновые уведомления о состоянии `telemt`.
- Telegram Mini App повторяет те же сценарии для пользователя и администратора в веб-интерфейсе внутри Telegram.
- Пользователями бот управляет через control API `telemt`, с fallback на legacy-конфиг и `systemd`. Работает и в Docker (`runtime.mode = "external"`).

Интерфейс устроен как у `BotFather`: slash-команда открывает inline-экран, дальше работают кнопки и короткие wizard-сценарии. Состояние wizard хранится в SQLite и переживает рестарт.

## Быстрый старт

На Linux с `systemd` bootstrap-скрипт ставит `telemt` и `telemt-admin`, создаёт конфиги и unit-файлы:

```bash
curl -fsSL https://raw.githubusercontent.com/fgbm/telemt-admin/master/scripts/install.sh | sudo bash
```

Docker, ручная установка, проверка после запуска и обновление описаны в [docs/installation.md](docs/installation.md).

> [!NOTE]
> Скрипт установки и Docker-образ `ghcr.io/fgbm/telemt-admin` берутся из upstream, функций форка в них нет. Чтобы их получить, соберите бинарник из этого репозитория: `cargo build --release` (см. [Сборка из исходников](docs/installation.md#сборка-из-исходников)). `telemt-admin self-update` тоже скачивает релиз upstream и заменит сборку форка на версию без его функций.

## Документация

- [Установка и обновление](docs/installation.md): требования, скрипт, Docker, systemd вручную, обновление, сборка, CLI.
- [Конфигурация](docs/configuration.md): все параметры `telemt-admin.toml` и переменные `TELEMT_ADMIN__*`.
- [Как пользоваться](docs/usage.md): сценарии пользователя, Mini App и админка.
- [Troubleshooting](docs/troubleshooting.md): частые ошибки и что проверить.
- [Runbook](docs/runbook.md): включение, проверка и откат API-first интеграции, observability-стек.
- [Архитектурные решения (ADR)](docs/adr/README.md), [обзор](docs/overview.md), [архитектура](docs/architecture.md), [разработка и CI](docs/development.md), [backlog](docs/backlog.md).
