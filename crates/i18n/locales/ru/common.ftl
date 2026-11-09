## Common actions and labels.
common-close = Закрыть
common-cancel = Отмена
common-save = Сохранить
common-retry = Повторить
common-search = Поиск
common-send = Отправить сообщение
common-attach = Прикрепить файл
common-remove-attachment = Удалить вложение
common-loading = Загрузка…
common-done = Готово
common-delete = Удалить
common-edit = Изменить

## Navigation and shell sections.
nav-chats = Чаты
nav-contacts = Контакты
nav-rooms = Комнаты
nav-feed = Лента
nav-communities = Сообщества
nav-sections = Разделы и пространства
nav-accounts = Аккаунты
nav-add-space = Добавить пространство
notify-always = Все сообщения
notify-mentions = Только упоминания
notify-never = Ничего
nav-settings = Настройки

## Language settings.
language-title = Язык
language-system = Системный по умолчанию
language-hint = Применяется ко всему приложению.

## Counted messages.
chat-typing = печатает…
chat-unread =
    { $count ->
        [one] { $count } непрочитанное сообщение
        [few] { $count } непрочитанных сообщения
        [many] { $count } непрочитанных сообщений
       *[other] { $count } непрочитанного сообщения
    }

space-members = Участники
space-notifications = Уведомления
space-address = Адрес

room-members =
    { $count ->
        [one] { $count } участник
        [few] { $count } участника
        [many] { $count } участников
       *[other] { $count } участника
    }

## Relative time.
time-now = сейчас
time-minutes-ago = { $count } мин назад
time-hours-ago = { $count } ч назад
time-days-ago = { $count } дн назад

## Application shell and authentication.
app-client = Нативный клиент XMPP
login-tagline = Ваши беседы без лишнего шума.
login-description = Быстрый и приватный клиент XMPP для компьютера. Общайтесь, заходите в комнаты, делитесь файлами и храните историю на своём устройстве.
login-direct = Личные сообщения, комнаты и архив
login-encrypted = Зашифрованные личные чаты с OMEMO
login-local = Локальное хранилище под вашим контролем
login-welcome = С возвращением
login-signin = Войти
login-subtitle = Используйте свой адрес XMPP и пароль.
common-jid = JID
common-password = Пароль
common-resource = Ресурс
login-insecure = Разрешать недействительные сертификаты TLS (только отладка)
login-remember = Запомнить этот аккаунт
login-connecting = Подключение…
login-password-note = Ваш пароль отправляется только на ваш сервер XMPP.
login-password-placeholder = Ваш пароль
login-error-jid = Введите полный JID, например user@example.com
login-error-password = Введите пароль

connection-connecting = Подключение к серверу…
connection-connecting-detail = Ожидание подключения XMPP и аутентификации.
connection-failed = Не удалось подключиться
connection-disconnected = Отключено
connection-disconnected-detail = Вы не подключены к серверу XMPP.

section-home = Личные сообщения и комнаты
section-contacts = Люди в вашем списке
section-rooms = Находите комнаты и управляйте ими
section-feed = Сообщества и публикации
section-communities = Подписывайтесь на сообщества и исследуйте их

nav-item-unread =
    { $label }, { $count ->
        [one] { $count } непрочитанное
        [few] { $count } непрочитанных
        [many] { $count } непрочитанных
       *[other] { $count } непрочитанного
    }

## Presence and account menus.
presence-title = Присутствие
presence-status = Статус
presence-online = В сети
presence-away = Отошёл
presence-idle = Неактивен
presence-dnd = Не беспокоить
presence-xa = Долгое отсутствие
presence-invisible = Невидимка
presence-custom = Своё сообщение
presence-custom-placeholder = Чем занимаетесь?
presence-priority = Приоритет
presence-mark-idle = Отметить как неактивного
presence-invisible-unsupported = Этот сервер не поддерживает режим невидимки.
presence-invisible-xep = Ваш сервер не поддерживает режим невидимки (XEP-0186)
presence-set-custom = Задать свой статус…
presence-edit-profile = Изменить профиль
presence-manage-accounts = Управление аккаунтами…
presence-gateways = Шлюзы
presence-sign-out = Выйти

## Accounts.
accounts-title = Аккаунты
accounts-add = Добавить аккаунт
accounts-add-connect = Добавить и подключить
accounts-active = Активный
accounts-online = В сети
accounts-offline = Не в сети
accounts-notify = Уведомлять
accounts-switch = Переключить
accounts-remove = Удалить
accounts-password-missing = Введите пароль аккаунта.
accounts-not-connected = Этот аккаунт не подключён.\nПерезапустите Jabu, чтобы запустить его.
accounts-no-config = У этого аккаунта нет сохранённой конфигурации.

## Settings.
settings-title = Настройки
settings-theme = Тема
theme-light = Светлая
theme-silver = Серебристая
theme-dark = Тёмная
theme-butter = Сливочная
settings-font-size = Размер шрифта
settings-compact = Компактные отступы
settings-autostart = Запускать при входе
settings-notifications = Уведомления на рабочем столе
settings-microphone = Микрофон
settings-camera = Камера
settings-sfu = SFU для звонков
settings-timestamps = Показывать время
settings-link-previews = Предпросмотр ссылок
settings-link-previews-hint = Загружает метаданные страниц по ссылкам, когда вы запрашиваете предпросмотр.
settings-shortcuts = Сочетания клавиш
settings-shortcut-search = Поиск
settings-shortcut-settings = Настройки
settings-shortcut-sections = Переключение раздела
settings-shortcut-focus = Фокус на беседе / боковой панели
settings-shortcut-close = Закрыть диалог или наложение
settings-shortcut-send = Отправить сообщение
settings-shortcut-menu = Открыть контекстное меню

## Conversations.
chat-no-conversations = Бесед пока нет
chat-no-archived = Нет архивных бесед
chat-unread-filter = Непрочитанные
chat-search-placeholder = Поиск бесед
chat-no-messages = Сообщений пока нет
chat-load-older = Загрузить более старые сообщения
chat-view-story = Посмотреть историю
chat-pin = Закрепить
chat-unpin = Открепить
chat-mute-action = Без звука
chat-unmute = Включить звук
chat-archive = В архив
chat-unarchive = Из архива
chat-forward = Переслать
chat-react = Реакция { $emoji }
chat-saved-to = Сохранено в { $path }
chat-unread-messages = { $count } непрочитанных сообщений
chat-empty-state = Выберите чат или комнату на боковой панели либо выполните поиск по сообщениям и контактам.
chat-pinned = Закреплено
chat-muted = Без звука
chat-online = В сети
chat-away = Отошёл
chat-dnd = Не беспокоить
chat-xa = Долгое отсутствие
chat-offline = Не в сети
chat-audio-call = Начать аудиозвонок
chat-video-call = Начать видеозвонок
chat-search-messages = Поиск сообщений
chat-clear-history = Очистить историю этой беседы
chat-change-subject = Изменить тему
chat-nudge = Отправить запрос внимания
chat-room-subject = Тема комнаты
chat-cancel-reply = Отменить ответ
chat-back = Назад
chat-sending = Отправка…
chat-failed = Ошибка
chat-edited = изменено
chat-encrypted = Зашифровано
chat-reply = Ответить
chat-reply-thread = Ответить в ветке
chat-in-thread = В ветке
chat-add-reaction = Добавить реакцию
chat-delete = Удалить
chat-delete-everyone = Удалить для всех
chat-download = Скачать в кеш вложений
chat-message = Сообщение
chat-send-placeholder = Отправьте сообщение…
chat-attach = Прикрепить файл
chat-send = Отправить сообщение
chat-send-sticker = Отправить стикер
chat-insert-emoji = Вставить эмодзи
chat-stop-upload = Остановить загрузку оставшихся файлов
chat-uploading = Загрузка…
chat-attach-short = Прикрепить
chat-searching = Поиск…
chat-type-more = Введите хотя бы два символа.
chat-no-results = Ничего не найдено.
chat-results-messages = Сообщения
chat-results-rooms = Сообщения комнат
chat-results-contacts = Контакты
chat-results-rooms-list = Комнаты
chat-results-posts = Публикации
chat-results-spaces = Пространства
chat-incoming-audio = Входящий аудиозвонок
chat-incoming-video = Входящий видеозвонок
chat-incoming-call = Входящий звонок
chat-decline = Отклонить
chat-accept = Принять
chat-voice-channel = Голосовой канал
chat-leave = Покинуть
chat-join = Присоединиться
chat-conn-screen = Подключение к экрану собеседника…
chat-starting-screen = Запуск демонстрации экрана…
chat-starting-camera = Запуск камеры…
chat-call-progress = Идёт звонок
chat-camera-on = Включить камеру
chat-camera-off = Выключить камеру
chat-camera-pending = Включение камеры…
chat-switch-camera = Переключить камеру
chat-share-screen = Показать экран
chat-stop-sharing = Остановить показ экрана
chat-hang-up = Завершить
chat-voice-connected = Голос подключён · { $count } в канале
chat-join-voice = Присоединитесь к голосовому каналу. Звук передаётся через SFU комнаты.
chat-join-voice-action = Войти в голос
chat-connecting = Подключение…
chat-mute-speaker = Выключить динамик
chat-unmute-speaker = Включить динамик
chat-incoming-from =
    { $kind ->
        [audio] Входящий аудиозвонок от { $name }
        [video] Входящий видеозвонок от { $name }
       *[other] Входящий звонок от { $name }
    }
chat-incoming-kind =
    { $kind ->
        [audio] Входящий аудиозвонок
        [video] Входящий видеозвонок
       *[other] Входящий звонок
    }
notify-new-story = Новая история
notify-new-post = Новая публикация
notify-story-title = 📝 Новая история от { $author }
mute-15-minutes = 15 минут
mute-1-hour = 1 час
mute-8-hours = 8 часов
mute-24-hours = 24 часа

## Space, social and picker surfaces.
space-category-name = Название категории
space-channel-name = Название канала
space-channel-type = Тип канала
space-category = Категория
space-change-avatar = Изменить аватар
space-change-banner = Изменить баннер
space-invite = Пригласить
space-configure = Настроить
space-members-todo = Управление участниками и настройка пока не подключены.
space-invite-link = Ссылка-приглашение
space-affiliation = Принадлежность
space-invite-help = Любой, у кого есть эта ссылка, может присоединиться к пространству. Только администраторы пространства могут предоставить принадлежность.
space-send-invite = Отправить приглашение
space-approve = Одобрить
space-deny = Отклонить
space-save-config = Сохранить конфигурацию
common-add = Добавить
space-save-directories = Сохранить каталоги
space-link-room = Привязать комнату
space-room-type = Тип комнаты
space-room-address = Адрес комнаты
space-display-name = Отображаемое имя
space-join-auto = Присоединяться автоматически
space-add-to = Добавить в пространство
space-visibility = Видимость
space-service-invalid = Адрес службы Spaces недействителен.
common-create = Создать
profile-full-name = Полное имя
profile-nickname = Псевдоним
profile-organisation = Организация
profile-title = Должность
profile-email = Эл. почта
profile-phone = Телефон
profile-website = Сайт
profile-note = Заметка
profile-avatar = Аватар
space-confirm-destroy = Подтвердить уничтожение
space-destroy = Уничтожить
chat-clear-help = Это удалит сообщения с этого устройства и из локального хранилища. Сообщения на сервере не удаляются.
chat-clear = Очистить
commands-title = Команды
common-next = Далее
common-finish = Завершить
common-submit = Отправить
contact-open-chat = Открыть чат
common-refresh = Обновить
report-action = Пожаловаться
contact-presence-sent = Запрос присутствия отправлен. Ожидание одобрения.
contact-request-presence = Запросить присутствие
gateway-register = Зарегистрироваться
contacts-requests = Запросы в контакты
contacts-none = Нет контактов
contacts-awaiting = Ожидает одобрения
contact-info = Информация
gateway-search-service = Поиск службы
common-load-more = Загрузить ещё
common-send-action = Отправить
common-new = Новый
feed-publish = Опубликовать
feed-new-post = Новая публикация
community-no-posts = В этом сообществе пока нет публикаций.
community-save = Сохранить настройки
community-delete = Удалить сообщество
feed-tags-placeholder = Теги (через запятую)
feed-load-older = Загрузить более старые публикации
story-share = Поделиться
story-open-media = Открыть медиа истории
story-encrypt-comment = Зашифровать комментарий с OMEMO
chat-encrypt = Зашифровать с OMEMO
story-retake = Переснять
story-replace = Заменить
story-take-photo = Сделать фото
story-cancel-camera = Отменить камеру
story-choose = Выберите изображение, видео или сделайте фото
story-gallery = Галерея
members-kick = Исключить
members-ban = Забанить
omemo-hide = Скрыть устройства шифрования
omemo-devices = Устройства шифрования
omemo-fetch = Получить ключи
omemo-heartbeat = Пульс
omemo-no-jids = Эта комната не раскрывает настоящие JID или не имеет известных участников.
omemo-none = Устройства OMEMO пока неизвестны. Включите шифрование в поле ввода, чтобы получить их.
omemo-trust = Доверять
omemo-distrust = Не доверять
sticker-load = Загрузить стикер
emoji-manage = Управление
emoji-help = Нажмите на эмодзи, чтобы добавить его в избранное или убрать оттуда.
emoji-remove-pack = Удалить набор
emoji-no-match = Подходящих эмодзи нет.
emoji-no-favorites = В избранном пока пусто. Нажмите «Избранное», чтобы добавить.
emoji-none = Здесь пока нет эмодзи.

## Reporting and stickers.
report-reason = Причина
report-spam = Спам
report-abuse = Оскорбления или домогательства
report-illegal = Незаконный контент
report-other = Другое
report-details = Подробности (необязательно)
report-send = Отправить жалобу
report-sending = Отправка…
report-stored = Жалоба сохранена. Id { $id }, статус { $status }.
stickers-title = Стикеры
stickers-import-hint = Импортировать набор стикеров (.zip)
stickers-import-action = Импортировать
stickers-no-packs = Наборов стикеров пока нет.
stickers-add-hint = Добавьте ZIP с изображениями или набор стикеров Movim с meta.json.
stickers-import-pack = Импортировать набор стикеров

## Remaining UI labels.
space-invitee-jid = JID приглашённого
profile-menu-title = Профиль
contact-jid = JID контакта
contact-nickname-placeholder = псевдоним (необязательно)
rooms-search-title = Поиск комнат
rooms-join = Войти в комнату
post-like = Нравится
post-comment = Комментировать
post-comment-placeholder = Напишите комментарий…
community-pubsub = Служба PubSub
community-create = Создать сообщество
community-node = Id узла
community-title = Название сообщества
feed-post = Опубликовать
feed-post-title = Заголовок публикации
feed-post-body = Текст публикации
feed-post-placeholder = Напишите что-нибудь…
feed-tags = Теги
community-label = Сообщество
community-description = Описание
community-members = Участники сообщества
report-this-post = Пожаловаться на публикацию
feed-edit = Изменить публикацию
feed-delete = Удалить публикацию
story-title = История
story-comment-placeholder = Отправить комментарий…
story-delete = Удалить эту историю
story-share-title = Поделиться историей
story-caption = Подпись
story-caption-placeholder = Добавить подпись…
main-no-conversation = Беседа не выбрана
main-conversation = Беседа
chat-nudge-action = Подтолкнуть
chat-preview-link = Предпросмотр ссылки
emoji-favorite = Добавить или убрать эмодзи из избранного
emoji-import = Импортировать набор эмодзи Movim (.zip)
common-import = Импортировать
emoji-search = Поиск эмодзи
community-pubsub-hint = Служба PubSub, например pubsub.example.org
community-node-hint = Id узла, например rust
emoji-alias-hint = Псевдоним в сообщениях, например :party:

feed-filter-all = Все
feed-post-hint = Поделитесь чем-нибудь с контактами…

## Dialog labels.
accounts-account-jid = JID аккаунта
call-connected = Подключено
space-menu = Меню пространства
channel-actions = Действия канала
space-create-category = Создать категорию
space-create-channel = Создать канал
space-members-tab = Участники
space-invite-title = Пригласить в пространство
space-members-title = Участники пространства
space-settings-title = Настройки пространства
space-rooms-title = Комнаты пространства
space-directory-placeholder = Название нового каталога
space-add-room-title = Добавить комнату
accounts-menu = Меню аккаунта
space-add-title = Добавить пространство
space-address-placeholder = Адрес пространства или приглашение
space-service-label = Служба Spaces
space-service-placeholder = Служба Spaces (автоматически, если пусто)
space-name = Название пространства
room-settings-title = Настройки комнаты
report-reason-placeholder = Причина (необязательно)
chat-clear-title = Очистить историю
rooms-search-public = Поиск публичных комнат
rooms-search-placeholder = Название комнаты или ключевое слово

call-authenticated = Аутентифицировано
call-reconnecting = Переподключение

report-title = Пожаловаться на контент

## Dates and numbers.
date-month-1 = января
date-month-2 = февраля
date-month-3 = марта
date-month-4 = апреля
date-month-5 = мая
date-month-6 = июня
date-month-7 = июля
date-month-8 = августа
date-month-9 = сентября
date-month-10 = октября
date-month-11 = ноября
date-month-12 = декабря
date-medium = { $day } { $month } { $year }
date-time = { $date }, { $time }
time-hm = { $hour }:{ $minute }

profile-listening = Слушает
profile-mood = Настроение
profile-activity = Занятие
profile-location = Местоположение
profile-idle-since = Неактивен с
profile-capabilities = Возможности
profile-subscriptions = Публичные подписки

## Calls and shell chrome.
call-calling = Вызов…
call-ringing = Звонит…
call-active = Идёт звонок
call-ended = Завершён
call-declined = Отклонён
call-missed = Пропущен
call-failed = Ошибка
call-incoming = Входящий
call-shared-screen = Общий экран
call-remote-screen = Удалённый экран
call-remote-video = Удалённое видео
call-screen-preview = Предпросмотр экрана
call-camera-preview = Предпросмотр камеры
tray-show = Показать Jabu
tray-quit = Выйти
shell-skip-content = Перейти к содержимому
titlebar-minimize = Свернуть
titlebar-maximize = Развернуть
titlebar-close = Закрыть
common-yes = Да
common-no = Нет
search-placeholder = Поиск сообщений, контактов и комнат
feed-open-hint = Ваша социальная лента открыта в основной области.

## Space rail and menus.
a11y-unread = { $count } непрочитанных
a11y-muted = без звука
a11y-pending-invite = ожидающее приглашение
a11y-locked = заблокировано
space-actions = Действия пространства
space-label = Пространство
space-channel = Канал
space-mark-read = Отметить прочитанным
space-pin = Закрепить пространство
space-unpin = Открепить пространство
space-unmute = Включить звук
space-muted-for = Без звука { $duration }
space-muted-forever = Без звука, пока вы не включите
space-until-unmute = Пока я не включу обратно
space-leave = Покинуть пространство
space-leave-confirm = Покинуть пространство?
space-leave-warning = Вы больше не увидите { $name } на панели. Сообщения останутся на сервере.
space-leaving = Выход…
space-info = Информация о пространстве
space-hide-muted = Скрыть каналы без звука
space-invite-menu = Пригласить в пространство…
space-share-link = Поделиться ссылкой на пространство
space-share-link-menu = Поделиться ссылкой на пространство…
space-create-category-menu = Создать категорию…
space-create-channel-menu = Создать канал…
space-link-room-menu = Привязать существующую комнату…
space-mute-channel = Отключить звук канала
space-unmute-channel = Включить звук канала
space-no-rooms = Комнат пока нет
space-all-hidden = Все каналы скрыты
space-unhide-hint = Покажите каналы без звука, чтобы снова их увидеть.
space-empty-hint = Комнаты, добавленные в это пространство, появятся здесь.
time-in-minutes = { $count } мин
time-in-hours = { $count } ч
time-in-days = { $count } дн

## Space and room dialogs.
common-creating = Создание…
common-subscribing = Подписка…
common-subscribe = Подписаться
common-discovering = Поиск…
common-saving = Сохранение…
common-optional = Необязательно
common-error-jid = Введите действительный JID.
space-error-category-name = Введите название категории.
space-category-placeholder = например Разработка
space-error-managers-only = Только администраторы пространства могут создавать каналы.
space-error-channel-name = Введите название канала.
space-error-room-exists = Комната с таким адресом уже существует в этом пространстве.
space-error-create-room = Не удалось создать комнату: { $error }
space-error-add-room = Комната создана, но её не удалось добавить в пространство: { $error }
space-channel-placeholder = например general
space-text-channel = Текстовый канал
space-voice-channel = Голосовой канал
space-no-category = Без категории
space-parent = Родительский
space-linked-nodes = Связанные узлы
space-related-nodes = Похожие узлы
space-related-hint = Дочерние или связанные узлы на этой службе
pubsub-invalid-service = недействительная служба PubSub
space-no-related = Похожие узлы не найдены
space-hierarchy = Иерархическая подписка
space-hierarchy-hint = Следить за этим узлом, его дочерними узлами или всем поддеревом
space-scope-node = Этот узел
space-scope-children = Дочерние
space-scope-all = Все потомки
space-scope-node-lower = этот узел
space-scope-children-lower = дочерние узлы
space-scope-all-lower = все потомки
space-subscribed = Подписка на { $scope }
space-role-member = Участник
space-role-admin = Администратор
space-role-owner = Владелец
space-role-outcast = Забанен
space-role-publisher = Издатель
space-role-none = Нет
space-affiliation-none = Без принадлежности
space-loading-members = Загрузка участников…
space-no-members = Служба не вернула участников.
members-pending = Ожидает
space-config-title = Конфигурация пространства
common-loading-config = Загрузка конфигурации…
space-rooms-dirs-title = Комнаты и каталоги пространства
space-directories = Каталоги
space-kind-text = Текст
space-kind-voice = Голос
space-no-directory = Без каталога
space-link-room-title = Привязать существующую комнату
space-voice-name-placeholder = Название голосового канала
space-error-room-address = Введите адрес комнаты.
space-error-room-address-invalid = Введите действительный адрес комнаты.
space-join-existing = Присоединиться к существующему пространству
space-error-invite = Введите действительную ссылку-приглашение.
space-create-new = Создать новое пространство
space-public = Публичное
space-private = Приватное
space-error-name = Введите название пространства.

## Chat labels.
chat-you = Вы
chat-attachment-fallback = [вложение]
chat-message-yourself = Написать себе
chat-note-to-self = Личная заметка
chat-back-to-chats = Назад к чатам
chat-forward-title = Переслать сообщение
chat-forward-failed = Не удалось переслать это сообщение.
chat-forward-none = Нет других бесед для пересылки.
room-error-jid = Недействительный JID комнаты
room-saved = Конфигурация комнаты сохранена.
room-destroyed = Комната уничтожена.
room-config-title = Конфигурация комнаты
room-error-invite-jid = Введите действительный JID для приглашения
room-invite-title = Пригласить в комнату
story-yours = Ваша история

## Space roles shown in member lists.
space-role-none-label = Нет

## Contacts, gateways and rooms.
presence-invisible-error = Этот сервер не поддерживает режим невидимки (XEP-0186).
calls-automatic = Автоматически
contact-idle = неактивен
contact-block = Заблокировать
contact-unblock = Разблокировать
contact-presence-wants = Этот контакт хочет видеть ваше присутствие.
contact-presence-mutual = Присутствие не передаётся в обе стороны.
contact-not-found = Контакт не найден
contact-groups = Группы (через запятую)
contacts-all = Все контакты
profile-name = Имя
profile-role = Роль
profile-no-subscriptions = Нет публичных подписок
profile-shared-media = Общие медиа
profile-no-media = Общих медиа пока нет
profile-resources = Ресурсы
profile-no-presence = Нет информации о присутствии
common-invalid-jid = Недействительный JID
common-removing = Удаление…
common-publishing = Публикация…
commands-adhoc-title = Ad-hoc команды
commands-still-running = Команда всё ещё выполняется.
commands-loading = Загрузка команд…
commands-none = Команды не объявлены
gateway-none = Этот сервер не объявляет шлюзы.
gateway-removed = Регистрация удалена.
gateway-submitted = Регистрация отправлена.
gateway-looking = Поиск шлюзов…
gateway-registered = Этот аккаунт зарегистрирован.
gateway-loading-form = Загрузка формы…
chat-attachment-name = Вложение
rooms-none = Вы не состоите ни в одной комнате
room-pin = Закрепить комнату
room-unpin = Открепить комнату
rooms-error-service-jid = Введите действительный JID службы поиска.
rooms-error-term = Введите поисковый запрос.
rooms-none-found = Публичные комнаты не найдены.
rooms-find = Найти публичные комнаты
rooms-not-open = Закрыта

## Communities and posts.
communities-explore = Исследовать сообщества
communities-following = Подписки
communities-none-following = Вы пока не подписаны ни на одно сообщество.
communities-cached = В кеше
communities-discover-hint = Находите публичные узлы PubSub и подписывайтесь на них.
communities-suggestions = Рекомендации
communities-suggested = Рекомендовано контактами
communities-none-cached = Сообществ в кеше пока нет. Найдите сервер PubSub выше.
community-error-required = Сервер, узел и название обязательны
community-articles = Статьи
community-create-action = Создать сообщество
community-follow = Подписаться
community-unfollow = Отписаться
community-settings-title = Настройки сообщества
community-policy-open = Публиковать может любой
community-policy-subscribers = Публиковать могут подписчики
community-policy-publishers = Только издатели
community-delete-confirm = Удалить это сообщество и все его публикации?
community-no-affiliation = Информация о принадлежности не получена.
feed-error-title = Добавьте заголовок публикации
feed-new-community-post = Новая публикация сообщества
feed-editing = Редактирование публикации
feed-tags-hint = rust, xmpp
feed-none = Публикаций пока нет
post-no-comments = Комментариев пока нет
post-edited = · изменено
story-play = Воспроизвести
story-pause = Пауза
story-unavailable = История недоступна
story-unavailable-short = Эта история больше недоступна.
story-unavailable-detail = Эта история больше недоступна. Возможно, она истекла или у вас нет доступа.
story-delete-confirm = Удалить эту историю?
story-delete-hint = Она будет немедленно отозвана из вашего узла историй.
story-no-share-targets = Нет контактов или комнат, с которыми можно поделиться.
story-error-image = Не удалось прочитать это изображение
story-error-media = Для истории нужно изображение или видео
story-error-no-camera = Камера не найдена
story-error-camera-image = Камера вернула нечитаемое изображение
story-error-crop = Не удалось обрезать это изображение
story-new = Новая история
story-crop-preview-alt = предпросмотр обрезки истории
story-zoom = Масштаб
story-horizontal = Горизонтально
story-vertical = Вертикально
story-camera-preview-alt = предпросмотр камеры
story-default-camera = Камера по умолчанию
story-publish = Опубликовать историю

## Chat message details and OMEMO.
chat-private-notes = Личные заметки
room-subject = Тема
room-no-occupants = Видимых участников пока нет.
room-kicked = Исключён
room-banned = Забанен
omemo-bundle-missing = набор ключей не загружен
omemo-trusted = Доверенное
omemo-untrusted = Не доверенное
omemo-unverified = Не проверено
omemo-signature-invalid = недействительная подпись
chat-sticker = Стикер
chat-forwarded = Пересланное сообщение
chat-mentioned-you = упомянул вас
chat-attention = Внимание
chat-edited-suffix = (изменено)
chat-upload-cancelled = Загрузка отменена
omemo-toggle = Переключить шифрование OMEMO
omemo-toggle-room = Переключить шифрование OMEMO комнаты

## Emoji picker and report dialog.
emoji-title = Эмодзи
emoji-recent = Недавние
emoji-favorites = Избранное
emoji-custom = Свои
emoji-tone = Оттенок
emoji-default-tone = По умолчанию
emoji-toggle-favorite = Переключить :{ $shortcut }: в избранном
emoji-tone-light = светлый оттенок кожи
emoji-tone-medium-light = средне-светлый оттенок кожи
emoji-tone-medium = средний оттенок кожи
emoji-tone-medium-dark = средне-тёмный оттенок кожи
emoji-tone-dark = тёмный оттенок кожи
emoji-group-smileys = Смайлы и эмоции
emoji-group-people = Люди и тело
emoji-group-animals = Животные и природа
emoji-group-food = Еда и напитки
emoji-group-travel = Путешествия и места
emoji-group-activities = Занятия
emoji-group-objects = Предметы
emoji-group-symbols = Символы
emoji-group-flags = Флаги
emoji-group-component = Компоненты
emoji-group-skin-tones = Оттенки кожи
report-target = Цель
report-no-host = Host для жалоб не настроен. Добавьте [reporting] host = "https://…" в config.toml.
stickers-invalid = Недействительные данные стикера
call-connecting = Подключение…

## Public Host pages.
host-nav-about = О проекте
host-footer-served = Обслуживается { $site }
host-footer-about = Об этом экземпляре
host-blog-description = Публичные публикации { $title }
host-blog-empty = Публичных публикаций пока нет.
host-by = Автор
host-post-fallback = Публикация
host-index-hint = Откройте публичный блог с помощью { $code }.
host-about-title = О { $site }
host-about-body = Этот экземпляр предоставляет только публичный контент XMPP. Чаты, приватные публикации и данные аккаунтов здесь никогда не раскрываются.
host-contact-description = Публичный профиль { $title }
host-contact-no-subscriptions = Нет публичных подписок.
host-public-blog = Публичный блог
host-public-subscriptions = Публичные подписки
host-report-page-title = Пожаловаться на страницу
host-report-description = Пожаловаться на публичную страницу в этом экземпляре.
host-target = Цель:
host-report-reason = Причина
host-report-details = Подробности
host-report-contact = Ваш контакт (необязательно)
host-report-note = Жалобы рассматривают модераторы этого экземпляра.
host-report-received = Жалоба получена
host-report-thanks = Спасибо. Id вашей жалобы: { $id }.
host-status = Статус:
host-report-check = Проверить статус позже
host-report-status-title = Статус жалобы
host-report-status-description = Статус публичной жалобы.
host-report-open = Ожидает рассмотрения.
host-report-hidden = Страница скрыта на этом экземпляре.
host-report-dismissed = Действий не предпринято.
host-report-created = Создано { $time }
host-not-found = Не найдено
host-not-found-detail = Эта страница не существует или её содержимое не является публичным.
host-community-description = Публичное сообщество { $title }
host-community-empty = Публичных статей пока нет.
host-community-label = Сообщество
host-space-description = Публичное пространство { $title }
host-space-rooms = Комнаты
host-space-empty = Публичные комнаты не указаны.
host-voice = голос
host-story = История
host-story-hint = История исчезает через 24 часа.
host-share = Поделиться:
host-open-in-jabu = Открыть в Jabu
host-report-page = Пожаловаться на эту страницу
host-article-fallback = Статья
host-unavailable = Публичный контент временно недоступен.

## Host admin pages.
host-admin-title = Администрирование
host-admin-description = Администрирование экземпляра.
host-admin-signin-title = Вход администратора
host-admin-token-label = Токен администратора
host-admin-signin = Войти
host-admin-settings = Настройки
host-admin-tokens = Токены
host-admin-backup = Резервная копия
host-admin-reporter = заявитель { $name }
host-admin-action-hide = Скрыть
host-admin-action-unhide = Показать
host-admin-action-dismiss = Отклонить
host-admin-action-reopen = Открыть снова
host-admin-note-placeholder = Заметка (необязательно)
host-admin-apply = Применить
host-admin-queue-title = Очередь модерации
host-admin-report-count = { $count } жалоб
host-admin-signout = Выйти
host-admin-no-reports = Жалоб нет.
host-admin-audit = Журнал аудита
host-admin-no-actions = Действий пока нет.
host-admin-moderation-title = Модерация
host-admin-moderation-description = Очередь жалоб и журнал аудита.
host-admin-settings-title = Настройки экземпляра
host-admin-site-name = Название сайта
host-admin-description-label = Описание
host-admin-accent = Акцентный цвет
host-admin-footer-note = Примечание в подвале
host-admin-footer-placeholder = Необязательный текст в подвале
host-admin-featured = JID избранных блогов, по одному в строке
host-admin-retention = Хранение (дней, 0 — хранить всегда)
host-admin-save-settings = Сохранить настройки
host-admin-back = Назад к модерации
host-admin-config-note = Адрес привязки, публичный URL, TTL кеша, ограничение частоты и путь к базе данных остаются в файле конфигурации.
host-admin-settings-description = Настройки и оформление экземпляра.
host-admin-new-token = Новый токен (скопируйте сейчас, он больше не будет показан):
host-admin-created = создан { $time }
host-admin-revoke = Отозвать
host-admin-tokens-title = Токены администратора
host-admin-create-title = Создать токен
host-admin-label = Метка
host-admin-label-placeholder = например android console
host-admin-role = Роль
host-admin-role-moderator = Модератор
host-admin-role-admin = Администратор
host-admin-create = Создать токен
host-admin-no-tokens = Токенов нет.
host-admin-tokens-description = Токены и роли администратора.
host-admin-invalid-token = Недействительный токен администратора
host-admin-settings-saved = Настройки сохранены.
host-admin-token-label-error = Метка токена должна содержать от 1 до 60 символов.
host-admin-unknown-role = Неизвестная роль
host-admin-status-open = открыта
host-admin-status-hidden = скрыта
host-admin-status-dismissed = отклонена
host-admin-site-required = Название сайта обязательно.
host-admin-site-too-long = Название сайта должно содержать не более { $count } символов.
host-admin-description-too-long = Описание должно содержать не более { $count } символов.
host-admin-footer-too-long = Примечание в подвале должно содержать не более { $count } символов.
host-admin-too-many-blogs = Допускается не более { $count } избранных блогов.
host-admin-invalid-jid = `{ $jid }` не является действительным JID.
host-admin-retention-too-long = Хранение должно быть от 0 до { $count } дней.
host-admin-accent-invalid = Акцентный цвет должен выглядеть как #7dd3fc.

## On-device translation.
settings-translation = Перевод
settings-translation-privacy = Перевод выполняется на этом устройстве. Текст сообщений никогда не отправляется на сервер.
settings-translation-enabled = Переводить сообщения
settings-translation-auto = Автоматически переводить входящие сообщения
settings-translation-target = Язык перевода
settings-translation-target-ui = Язык интерфейса
settings-translation-model = Модель
settings-translation-no-model = Модель не установлена
settings-translation-download = Скачать
settings-translation-downloading = Загрузка…
settings-translation-installed = Установлено
settings-translation-restart = Перезапустите Jabu, чтобы использовать только что установленную модель.
chat-translate = Перевести
chat-translating = Перевод…
chat-translation-failed = Не удалось перевести
chat-machine-translation = Машинный перевод
chat-show-original = Показать оригинал
chat-hide-original = Скрыть оригинал
chat-auto-translate = Автоматически переводить этот чат
