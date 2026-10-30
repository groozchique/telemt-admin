# ADR 001: API-first backend для `telemt`

## Статус

Принято

## Контекст

Изначально `telemt-admin` управлял `telemt` через два низкоуровневых механизма:

- прямое редактирование `telemt.toml` через `src/telemt_cfg.rs`;
- применение изменений через `systemd` в `src/service.rs`.

Разбор `../telemt` показал, что у прокси уже есть control API с основными операциями администрирования:

- health/status;
- runtime/system info;
- CRUD пользователей;
- готовые `tg://proxy` ссылки;
- optimistic concurrency через `revision` и `If-Match`.

При этом полный отказ от старого пути на первом шаге был бы рискованным:

- rollout должен оставаться обратимым;
- часть окружений может ещё не иметь настроенного `[server.api]`;
- в случае API-сбоя бот не должен терять работоспособность полностью.

## Решение

В проекте вводится единый backend-слой `src/telemt_backend.rs` с режимом `API-first`.

Публичный фасад backend-слоя сохраняется в `src/telemt_backend.rs`, а внутренняя реализация может быть разнесена по подмодулям (`api_client`, DTO, legacy fallback, mapping), если это уменьшает связность без изменения внешнего контракта.

Он инкапсулирует выбор между:

- control API `telemt`;
- legacy file/systemd path.

Выбор backend-режима конфигурируется через секцию `[telemt_api]` в `telemt-admin.toml`.

Telegram handlers и action-слой не должны обращаться напрямую к:

- `telemt_cfg`;
- `service`;
- HTTP-деталям `telemt`.

Они работают только через единый backend-контракт.

## Последствия

Положительные:

- use-case слой больше не знает, как именно применяется операция в `telemt`;
- rollout можно делать поэтапно и с откатом;
- link retrieval и runtime status можно брать напрямую из API;
- логика retry/revision/fallback сосредоточена в одном месте.

Негативные:

- временно поддерживаются два пути интеграции вместо одного;
- увеличивается объём конфигурации и документации;
- требуется аккуратно документировать, какой путь является источником истины в каждом сценарии.

## Инварианты

- при включённом `[telemt_api].enabled = true` backend сначала пробует control API;
- при включённом `allow_file_fallback = true` допустим возврат к legacy-пути;
- handlers не должны встраивать HTTP-вызовы к `telemt` напрямую;
- `telemt_backend` остаётся единой точкой оркестрации работы с proxy backend.

## Отложенные решения

- отказ от legacy fallback после стабилизации production rollout;
- дальнейшее упрощение backend-слоя после стабилизации rollout, если появятся новые поддомены (`metrics`, reconciliation, sync-state policy);
- использование `PATCH` и `rotate-secret` после исправления серверной стороны в `telemt`.
