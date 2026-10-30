# ADR 003: runtime-agnostic развёртывание (systemd / Docker / без unit)

## Статус

Принято

## Контекст

Проект изначально ориентирован на Linux с `systemd`: установщик и Polkit, прямые вызовы `systemctl`/`journalctl`, legacy-путь с `HUP` после правки `telemt.toml`.

Сообщество запрашивает контейнерный запуск и универсальный стек (`telemt`, UI, бот). Внутри контейнера обычно нет полноценного управления **хостовым** `telemt.service` через D-Bus/systemd без нестандартных привилегий.

## Решение

1. Ввести слой **`TelemtRuntime`** (`src/runtime/`) с режимами:
   - `systemd` — текущее поведение через `src/service.rs` (`systemctl`, `journalctl`);
   - `external` — процесс telemt управляется снаружи (Docker Compose, k8s и т.д.): без lifecycle-кнопок, статус и диагностика через control API;
   - `none` — то же, что `external`, без пользовательской метки.

2. Конфигурация через опциональную секцию **`[runtime]`** в `telemt-admin.toml`. Если секция отсутствует, сохраняется прежняя семантика: `mode = systemd`, unit из верхнеуровневого `service_name`.

3. UI экрана `/service` и inline-клавиатура зависят от **`RuntimeCapabilities`**: кнопки start/stop/restart/reload показываются только в режиме systemd.

4. Legacy fallback по-прежнему вызывает `notify_config_reloaded()` на runtime-слое; в режимах `external`/`none` это возвращает понятную ошибку (управление только вручную).

5. Docker-образ собирает только бинарник, без systemd внутри; пример compose — в `deploy/compose/`.

## Последствия

- Контейнерный сценарий опирается на **`[telemt_api]`**; рекомендуется `allow_file_fallback = false` в Docker.
- Эксплуатация на голом Linux + systemd остаётся основным путём без изменения поведения при отсутствии `[runtime]`.
- Полный стек из нескольких сервисов задаётся оркестратором (compose и т.д.), а не встроенным `systemd` внутри образа бота.

## Инварианты

- `telemt_backend` остаётся единой точкой выбора API vs legacy file; runtime не дублирует HTTP к telemt.
- Управление жизненным циклом процесса telemt на хосте не смешивается с доменными операциями над пользователями через API.
