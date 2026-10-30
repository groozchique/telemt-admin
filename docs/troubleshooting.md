# Troubleshooting

- `Не задан bot_token...`  
  Укажите `bot_token` в конфиге, либо `TELEMT_ADMIN__BOT_TOKEN`, либо `TELOXIDE_TOKEN` в окружении процесса.

- Нет ссылки на invite-токен / «username бота неизвестен»  
  Укажите `bot_username` в `telemt-admin.toml` или `TELEMT_ADMIN__BOT_USERNAME` в окружении, если исходящий доступ к Telegram API для `getMe` отсутствует или у бота в профиле не задан публичный username.

- `telemt API error ...`  
  Проверьте, что в `telemt` включён `[server.api]`, bind доступен с машины бота, а `telemt_api.auth_header` в `telemt-admin.toml` в точности совпадает с `server.api.auth_header`.

- `Permission denied` при записи в `/etc/telemt.toml`  
  Это важно только для fallback-режима. Проверьте группу/права файла и что пользователь `telemt-admin` входит в нужную группу `telemt`.

- Не удаётся выполнить `/service restart` или автоматическое применение конфига  
  Проверьте правило Polkit для `org.freedesktop.systemd1.manage-units`, корректность `service_name` и что у пользователя сервиса есть доступ к `telemt.service`.

- Бот не отвечает на команды  
  Проверьте логи `journalctl -u telemt-admin.service` и валидность токена бота.

- Нет кнопки Mini App, в логе `Mini App: не удалось открыть порт` или `кнопка меню сброшена`  
  Порт из `webapp.listen` занят либо `webapp.public_url` не задан или не HTTPS. Исправьте конфиг и перезапустите сервис; бот при этом работает без Mini App. Клиент Telegram может показать новую кнопку меню не сразу — откройте чат заново.

- Mini App: «Откройте приложение из Telegram» или «Сессия недействительна, откройте приложение заново» (HTTP 401)  
  Запрос пришёл без подписи `initData`, подпись не сошлась (другой токен бота) или сессия старше `init_data_max_age_secs`. Закройте и заново откройте приложение из Telegram; проверьте время на сервере (NTP).

- Mini App: «Доступно только администраторам» (HTTP 403)  
  Telegram ID пользователя не входит в `admin_ids`.

- Mini App: «telemt API недоступен» (HTTP 502) или «Активность доступна только в режиме control API telemt» (HTTP 501)  
  Проверьте `[telemt_api]` и доступность control API `telemt` с машины бота.

- WEB-ссылка не подключается  
  Без `[web_proxy] manage_profiles` бот не создаёт WEB-профили в `telemt`: у пользователя должен быть профиль в `[[web.vhosts.profiles]]` с тем же `secret_mode`, что в `[web_proxy]`, а `[web_proxy] host` и `base_path` должны совпадать с `host` и `base_path` vhost WEB-прокси.
