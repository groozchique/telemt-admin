<p align="center">
  <img src="logo.png" alt="telemt-admin logo" width="800">
</p>

<h1 align="center">telemt-admin</h1>

**telemt-admin** — это Telegram-бот для автоматизации управления пользователями [telemt](https://github.com/telemt/telemt) (MTProto прокси-сервера). Проект создан для системных администраторов, которым нужно делегировать процесс выдачи доступа, избавившись от ручного редактирования конфигурационных файлов.

Поддерживает **систему пригласительных токенов** с двумя режимами работы:

- **Ручной (Manual):** пользователь подает заявку, админ подтверждает.
- **Автоматический (Auto-approve):** пользователь мгновенно получает доступ без участия админа.

**Семантика invite-токена:** при создании токена задаётся **срок действия ссылки** (в днях) и **лимит активаций** (сколько раз по ссылке можно зарегистрироваться). Это **не** срок подписки и **не** автоматическое ограничение доступа пользователя в `telemt` после регистрации. Дату истечения доступа пользователя, квоты трафика и прочие лимиты настраивают **отдельно** в карточке пользователя в боте (через control API `telemt`).

## Что добавляет этот форк

Форк [fgbm/telemt-admin](https://github.com/fgbm/telemt-admin). Всё, что есть в upstream, работает так же; сверху добавлено:

- **WEB-прокси с `base_path`.** `[web_proxy] base_path` — бот выдаёт path-ссылку в формате `telemt` (`server=<host>%2F<base_path>`, секрет base64url), если WEB-прокси висит не в корне vhost.
- **Вид WEB-ссылки.** `[web_proxy] link_scheme = "tg" | "https"` — `tg://webproxy?…` или `https://t.me/webproxy?…`.
- **Автоматические WEB-профили.** `[web_proxy] manage_profiles = true` — при одобрении бот сам добавляет пользователю профиль в `[[web.vhosts.profiles]]` через control API и делает reload `telemt` с drain (`reload_drain_secs`). Перед удалением пользователя профили снимаются всегда, иначе `telemt` отклоняет удаление. Подробности — [ADR 006](docs/adr/006-web-profiles-management.md).
- **Обратная связь.** Кнопка «Обратная связь» у пользователя в боте и Mini App: сообщение (текст, медиа, альбом) копируется всем `admin_ids` с кнопкой «↩️ Ответить», ответ приходит пользователю от имени бота. Тексты — `feedback_*` и `admin_feedback_template` в `[bot_messages]`.
- **Пожертвования.** Кнопка «💝 Пожертвование» с текстом из `[bot_messages] donation_text`; без текста кнопка скрыта.

## Возможности

- **Для пользователей:** регистрация через бота по пригласительному токену, ссылка и QR, WEB-ссылка `tg://webproxy?…` для Telegram Desktop 7.1+ (`telemt` 3.5+).
- **Для администраторов:** заявки, invite-токены, карточки пользователей с live-данными и лимитами, группы, рассылка, импорт из `telemt`, панель `/service`, фоновые уведомления о состоянии `telemt`.
- **Telegram Mini App:** веб-интерфейс внутри Telegram с теми же сценариями для пользователя и администратора.
- **Для сервера:** управление пользователями через control API `telemt` с fallback на legacy-конфиг и `systemd`; работает и в Docker (`runtime.mode = "external"`).

Интерфейс построен по модели `BotFather`: slash-команда открывает inline-экран, действия — кнопками и короткими wizard-сценариями; состояние wizard хранится в SQLite и переживает рестарт.

## Быстрый старт

Linux с `systemd` — bootstrap-скрипт ставит `telemt` и `telemt-admin`, создаёт конфиги и unit-файлы:

```bash
curl -fsSL https://raw.githubusercontent.com/fgbm/telemt-admin/master/scripts/install.sh | sudo bash
```

Docker, ручная установка, проверка после запуска и обновление — в [docs/installation.md](docs/installation.md).

> [!NOTE]
> Скрипт установки и Docker-образ `ghcr.io/fgbm/telemt-admin` берутся из upstream и функций форка не содержат. Чтобы их получить, соберите бинарник из этого репозитория: `cargo build --release` (см. [Сборка из исходников](docs/installation.md#сборка-из-исходников)).

## Документация

- [Установка и обновление](docs/installation.md) — требования, скрипт, Docker, systemd вручную, обновление, сборка, CLI.
- [Конфигурация](docs/configuration.md) — все параметры `telemt-admin.toml` и переменные `TELEMT_ADMIN__*`.
- [Как пользоваться](docs/usage.md) — сценарии пользователя, Mini App и админка.
- [Troubleshooting](docs/troubleshooting.md) — частые ошибки и что проверить.
- [Runbook](docs/runbook.md) — включение, проверка и откат API-first интеграции, observability-стек.
- [Архитектурные решения (ADR)](docs/adr/README.md), [обзор](docs/overview.md), [архитектура](docs/architecture.md), [разработка и CI](docs/development.md), [backlog](docs/backlog.md).
