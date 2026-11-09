## Common actions and labels.
common-close = Zamknij
common-cancel = Anuluj
common-save = Zapisz
common-retry = Ponów
common-search = Szukaj
common-send = Wyślij wiadomość
common-attach = Dołącz plik
common-remove-attachment = Usuń załącznik
common-loading = Ładowanie…
common-done = Gotowe
common-delete = Usuń
common-edit = Edytuj

## Navigation and shell sections.
nav-chats = Czaty
nav-contacts = Kontakty
nav-rooms = Pokoje
nav-feed = Aktualności
nav-communities = Społeczności
nav-sections = Sekcje i przestrzenie
nav-accounts = Konta
nav-add-space = Dodaj przestrzeń
notify-always = Wszystkie wiadomości
notify-mentions = Tylko wzmianki
notify-never = Nic
nav-settings = Ustawienia

## Language settings.
language-title = Język
language-system = Domyślny systemowy
language-hint = Dotyczy całej aplikacji.

## Counted messages.
chat-typing = pisze…
chat-unread =
    { $count ->
        [one] { $count } nieprzeczytana wiadomość
        [few] { $count } nieprzeczytane wiadomości
        [many] { $count } nieprzeczytanych wiadomości
       *[other] { $count } nieprzeczytanej wiadomości
    }

space-members = Członkowie
space-notifications = Powiadomienia
space-address = Adres

room-members =
    { $count ->
        [one] { $count } członek
        [few] { $count } członków
        [many] { $count } członków
       *[other] { $count } członka
    }

## Relative time.
time-now = teraz
time-minutes-ago = { $count } min temu
time-hours-ago = { $count } godz. temu
time-days-ago = { $count } dni temu

## Application shell and authentication.
app-client = Natywny klient XMPP
login-tagline = Twoje rozmowy, bez szumu.
login-description = Szybki i prywatny klient XMPP na komputer. Rozmawiaj, dołączaj do pokoi, udostępniaj pliki i trzymaj historię na własnym urządzeniu.
login-direct = Wiadomości bezpośrednie, pokoje i archiwum
login-encrypted = Szyfrowane rozmowy 1:1 z OMEMO
login-local = Lokalny magazyn pod Twoją kontrolą
login-welcome = Witaj ponownie
login-signin = Zaloguj się
login-subtitle = Użyj swojego adresu XMPP i hasła.
common-jid = JID
common-password = Hasło
common-resource = Zasób
login-insecure = Zezwalaj na nieprawidłowe certyfikaty TLS (tylko debugowanie)
login-remember = Zapamiętaj to konto
login-connecting = Łączenie…
login-password-note = Hasło jest wysyłane tylko do serwera XMPP.
login-password-placeholder = Twoje hasło
login-error-jid = Podaj pełny JID, np. user@example.com
login-error-password = Podaj hasło

connection-connecting = Łączenie z serwerem…
connection-connecting-detail = Oczekiwanie na połączenie XMPP i uwierzytelnienie.
connection-failed = Nie można połączyć
connection-disconnected = Rozłączono
connection-disconnected-detail = Nie jesteś połączony z serwerem XMPP.

section-home = Wiadomości bezpośrednie i pokoje
section-contacts = Osoby z Twojej listy
section-rooms = Odkrywaj pokoje i zarządzaj nimi
section-feed = Społeczności i posty
section-communities = Obserwuj społeczności i odkrywaj je

nav-item-unread =
    { $label }, { $count ->
        [one] { $count } nieprzeczytana
        [few] { $count } nieprzeczytane
        [many] { $count } nieprzeczytanych
       *[other] { $count } nieprzeczytanej
    }

## Presence and account menus.
presence-title = Obecność
presence-status = Status
presence-online = Online
presence-away = Z dala
presence-idle = Bezczynny
presence-dnd = Nie przeszkadzać
presence-xa = Długo nieobecny
presence-invisible = Niewidoczny
presence-custom = Własna wiadomość
presence-custom-placeholder = Co robisz?
presence-priority = Priorytet
presence-mark-idle = Oznacz jako bezczynny
presence-invisible-unsupported = Ten serwer nie obsługuje trybu niewidocznego.
presence-invisible-xep = Twój serwer nie obsługuje trybu niewidocznego (XEP-0186)
presence-set-custom = Ustaw własny status…
presence-edit-profile = Edytuj profil
presence-manage-accounts = Zarządzaj kontami…
presence-gateways = Bramy
presence-sign-out = Wyloguj

## Accounts.
accounts-title = Konta
accounts-add = Dodaj konto
accounts-add-connect = Dodaj i połącz
accounts-active = Aktywne
accounts-online = Online
accounts-offline = Offline
accounts-notify = Powiadamiaj
accounts-switch = Przełącz
accounts-remove = Usuń
accounts-password-missing = Podaj hasło konta.
accounts-not-connected = To konto nie jest połączone.\nUruchom Jabu ponownie, aby je uruchomić.
accounts-no-config = To konto nie ma zapisanej konfiguracji.

## Settings.
settings-title = Ustawienia
settings-theme = Motyw
theme-light = Jasny
theme-silver = Srebrny
theme-dark = Ciemny
theme-butter = Maślany
settings-font-size = Rozmiar czcionki
settings-compact = Kompaktowe odstępy
settings-autostart = Uruchamiaj przy logowaniu
settings-notifications = Powiadomienia pulpitu
settings-microphone = Mikrofon
settings-camera = Kamera
settings-sfu = SFU rozmów
settings-timestamps = Pokaż znaczniki czasu
settings-link-previews = Podglądy linków
settings-link-previews-hint = Pobiera metadane stron z linkowanych witryn, gdy poprosisz o podgląd.
settings-shortcuts = Skróty klawiszowe
settings-shortcut-search = Szukaj
settings-shortcut-settings = Ustawienia
settings-shortcut-sections = Przełącz sekcję
settings-shortcut-focus = Fokus na rozmowie / pasku bocznym
settings-shortcut-close = Zamknij okno lub nakładkę
settings-shortcut-send = Wyślij wiadomość
settings-shortcut-menu = Otwórz menu kontekstowe

## Conversations.
chat-no-conversations = Brak rozmów
chat-no-archived = Brak zarchiwizowanych rozmów
chat-unread-filter = Nieprzeczytane
chat-search-placeholder = Szukaj rozmów
chat-no-messages = Brak wiadomości
chat-load-older = Wczytaj starsze wiadomości
chat-view-story = Zobacz historię
chat-pin = Przypnij
chat-unpin = Odepnij
chat-mute-action = Wycisz
chat-unmute = Wyłącz wyciszenie
chat-archive = Archiwizuj
chat-unarchive = Przywróć z archiwum
chat-forward = Przekaż dalej
chat-react = Zareaguj { $emoji }
chat-saved-to = Zapisano w { $path }
chat-unread-messages = { $count } nieprzeczytanych wiadomości
chat-empty-state = Wybierz czat lub pokój z paska bocznego albo przeszukaj wiadomości i kontakty.
chat-pinned = Przypięte
chat-muted = Wyciszone
chat-online = Online
chat-away = Z dala
chat-dnd = Nie przeszkadzać
chat-xa = Długo nieobecny
chat-offline = Offline
chat-audio-call = Rozpocznij rozmowę audio
chat-video-call = Rozpocznij rozmowę wideo
chat-search-messages = Szukaj wiadomości
chat-clear-history = Wyczyść historię tej rozmowy
chat-change-subject = Zmień temat
chat-nudge = Wyślij prośbę o uwagę
chat-room-subject = Temat pokoju
chat-cancel-reply = Anuluj odpowiedź
chat-back = Wstecz
chat-sending = Wysyłanie…
chat-failed = Niepowodzenie
chat-edited = edytowano
chat-encrypted = Zaszyfrowane
chat-reply = Odpowiedz
chat-reply-thread = Odpowiedz w wątku
chat-in-thread = W wątku
chat-add-reaction = Dodaj reakcję
chat-delete = Usuń
chat-delete-everyone = Usuń dla wszystkich
chat-download = Pobierz do pamięci podręcznej załączników
chat-message = Wiadomość
chat-send-placeholder = Wyślij wiadomość…
chat-attach = Dołącz plik
chat-send = Wyślij wiadomość
chat-send-sticker = Wyślij naklejkę
chat-insert-emoji = Wstaw emoji
chat-stop-upload = Zatrzymaj wysyłanie pozostałych plików
chat-uploading = Wysyłanie…
chat-attach-short = Dołącz
chat-searching = Szukanie…
chat-type-more = Wpisz co najmniej dwa znaki.
chat-no-results = Brak wyników.
chat-results-messages = Wiadomości
chat-results-rooms = Wiadomości z pokoi
chat-results-contacts = Kontakty
chat-results-rooms-list = Pokoje
chat-results-posts = Posty
chat-results-spaces = Przestrzenie
chat-incoming-audio = Przychodzące połączenie audio
chat-incoming-video = Przychodzące połączenie wideo
chat-incoming-call = Przychodzące połączenie
chat-decline = Odrzuć
chat-accept = Odbierz
chat-voice-channel = Kanał głosowy
chat-leave = Opuść
chat-join = Dołącz
chat-conn-screen = Łączenie z ekranem drugiej osoby…
chat-starting-screen = Uruchamianie udostępniania ekranu…
chat-starting-camera = Uruchamianie kamery…
chat-call-progress = Trwa połączenie
chat-camera-on = Włącz kamerę
chat-camera-off = Wyłącz kamerę
chat-camera-pending = Włączanie kamery…
chat-switch-camera = Przełącz kamerę
chat-share-screen = Udostępnij ekran
chat-stop-sharing = Zatrzymaj udostępnianie ekranu
chat-hang-up = Rozłącz
chat-voice-connected = Głos połączony · { $count } w kanale
chat-join-voice = Dołącz do kanału głosowego. Dźwięk jest przekazywany przez SFU pokoju.
chat-join-voice-action = Dołącz do głosu
chat-connecting = Łączenie…
chat-mute-speaker = Wycisz głośnik
chat-unmute-speaker = Wyłącz wyciszenie głośnika
chat-incoming-from =
    { $kind ->
        [audio] Przychodzące połączenie audio od { $name }
        [video] Przychodzące połączenie wideo od { $name }
       *[other] Przychodzące połączenie od { $name }
    }
chat-incoming-kind =
    { $kind ->
        [audio] Przychodzące połączenie audio
        [video] Przychodzące połączenie wideo
       *[other] Przychodzące połączenie
    }
notify-new-story = Nowa historia
notify-new-post = Nowy post
notify-story-title = 📝 Nowa historia od { $author }
mute-15-minutes = 15 minut
mute-1-hour = 1 godzina
mute-8-hours = 8 godzin
mute-24-hours = 24 godziny

## Space, social and picker surfaces.
space-category-name = Nazwa kategorii
space-channel-name = Nazwa kanału
space-channel-type = Typ kanału
space-category = Kategoria
space-change-avatar = Zmień awatar
space-change-banner = Zmień baner
space-invite = Zaproś
space-configure = Konfiguruj
space-members-todo = Zarządzanie członkami i konfiguracja nie są jeszcze podłączone.
space-invite-link = Link zaproszenia
space-affiliation = Przynależność
space-invite-help = Każdy z tym linkiem może dołączyć do przestrzeni. Tylko administratorzy przestrzeni mogą przyznać przynależność.
space-send-invite = Wyślij zaproszenie
space-approve = Zatwierdź
space-deny = Odrzuć
space-save-config = Zapisz konfigurację
common-add = Dodaj
space-save-directories = Zapisz katalogi
space-link-room = Połącz pokój
space-room-type = Typ pokoju
space-room-address = Adres pokoju
space-display-name = Nazwa wyświetlana
space-join-auto = Dołącz automatycznie
space-add-to = Dodaj do przestrzeni
space-visibility = Widoczność
space-service-invalid = Adres usługi Spaces jest nieprawidłowy.
common-create = Utwórz
profile-full-name = Pełne imię i nazwisko
profile-nickname = Pseudonim
profile-organisation = Organizacja
profile-title = Stanowisko
profile-email = E-mail
profile-phone = Telefon
profile-website = Strona internetowa
profile-note = Notatka
profile-avatar = Awatar
space-confirm-destroy = Potwierdź zniszczenie
space-destroy = Zniszcz
chat-clear-help = Spowoduje to usunięcie wiadomości z tego urządzenia i lokalnego magazynu. Wiadomości na serwerze nie zostaną usunięte.
chat-clear = Wyczyść
commands-title = Polecenia
common-next = Dalej
common-finish = Zakończ
common-submit = Wyślij
contact-open-chat = Otwórz czat
common-refresh = Odśwież
report-action = Zgłoś
contact-presence-sent = Wysłano prośbę o obecność. Oczekiwanie na zatwierdzenie.
contact-request-presence = Poproś o obecność
gateway-register = Zarejestruj
contacts-requests = Prośby o kontakt
contacts-none = Brak kontaktów
contacts-awaiting = Oczekuje na zatwierdzenie
contact-info = Informacje
gateway-search-service = Szukaj usługi
common-load-more = Wczytaj więcej
common-send-action = Wyślij
common-new = Nowy
feed-publish = Opublikuj post
feed-new-post = Nowy post
community-no-posts = Ta społeczność nie ma jeszcze postów.
community-save = Zapisz ustawienia
community-delete = Usuń społeczność
feed-tags-placeholder = Tagi (oddzielone przecinkami)
feed-load-older = Wczytaj starsze posty
story-share = Udostępnij
story-open-media = Otwórz media historii
story-encrypt-comment = Zaszyfruj komentarz za pomocą OMEMO
chat-encrypt = Szyfruj za pomocą OMEMO
story-retake = Zrób ponownie
story-replace = Zastąp
story-take-photo = Zrób zdjęcie
story-cancel-camera = Anuluj kamerę
story-choose = Wybierz obraz, wideo lub zrób zdjęcie
story-gallery = Galeria
members-kick = Wyrzuć
members-ban = Zbanuj
omemo-hide = Ukryj urządzenia szyfrujące
omemo-devices = Urządzenia szyfrujące
omemo-fetch = Pobierz klucze
omemo-heartbeat = Puls
omemo-no-jids = Ten pokój nie ujawnia prawdziwych JID-ów lub nie ma znanych członków.
omemo-none = Nie znasz jeszcze żadnych urządzeń OMEMO. Włącz szyfrowanie w edytorze, aby je pobrać.
omemo-trust = Zaufaj
omemo-distrust = Nie ufaj
sticker-load = Wczytaj naklejkę
emoji-manage = Zarządzaj
emoji-help = Dotknij emoji, aby dodać je do Ulubionych lub z nich usunąć.
emoji-remove-pack = Usuń pakiet
emoji-no-match = Brak pasującego emoji.
emoji-no-favorites = Brak ulubionych. Dotknij Ulubione, aby coś dodać.
emoji-none = Nie ma tu jeszcze emoji.

## Reporting and stickers.
report-reason = Powód
report-spam = Spam
report-abuse = Nadużycie lub nękanie
report-illegal = Nielegalna treść
report-other = Inne
report-details = Szczegóły (opcjonalnie)
report-send = Wyślij zgłoszenie
report-sending = Wysyłanie…
report-stored = Zgłoszenie zapisane. Id { $id }, status { $status }.
stickers-title = Naklejki
stickers-import-hint = Importuj pakiet naklejek (.zip)
stickers-import-action = Importuj
stickers-no-packs = Brak pakietów naklejek.
stickers-add-hint = Dodaj ZIP z obrazami lub pakiet naklejek Movim z meta.json.
stickers-import-pack = Importuj pakiet naklejek

## Remaining UI labels.
space-invitee-jid = JID zapraszanego
profile-menu-title = Profil
contact-jid = JID kontaktu
contact-nickname-placeholder = pseudonim (opcjonalnie)
rooms-search-title = Szukaj pokoi
rooms-join = Dołącz do pokoju
post-like = Lubię to
post-comment = Skomentuj
post-comment-placeholder = Napisz komentarz…
community-pubsub = Usługa PubSub
community-create = Utwórz społeczność
community-node = Id węzła
community-title = Tytuł społeczności
feed-post = Opublikuj
feed-post-title = Tytuł posta
feed-post-body = Treść posta
feed-post-placeholder = Napisz coś…
feed-tags = Tagi
community-label = Społeczność
community-description = Opis
community-members = Członkowie społeczności
report-this-post = Zgłoś ten post
feed-edit = Edytuj post
feed-delete = Usuń post
story-title = Historia
story-comment-placeholder = Wyślij komentarz…
story-delete = Usuń tę historię
story-share-title = Udostępnij historię
story-caption = Podpis
story-caption-placeholder = Dodaj podpis…
main-no-conversation = Nie wybrano rozmowy
main-conversation = Rozmowa
chat-nudge-action = Szturchnij
chat-preview-link = Podgląd linku
emoji-favorite = Dodaj lub usuń ulubione emoji
emoji-import = Importuj pakiet emoji Movim (.zip)
common-import = Importuj
emoji-search = Szukaj emoji
community-pubsub-hint = Usługa PubSub, np. pubsub.example.org
community-node-hint = Id węzła, np. rust
emoji-alias-hint = Alias używany w wiadomościach, np. :party:

feed-filter-all = Wszystko
feed-post-hint = Udostępnij coś swoim kontaktom…

## Dialog labels.
accounts-account-jid = JID konta
call-connected = Połączono
space-menu = Menu przestrzeni
channel-actions = Działania kanału
space-create-category = Utwórz kategorię
space-create-channel = Utwórz kanał
space-members-tab = Członkowie
space-invite-title = Zaproś do przestrzeni
space-members-title = Członkowie przestrzeni
space-settings-title = Ustawienia przestrzeni
space-rooms-title = Pokoje przestrzeni
space-directory-placeholder = Nazwa nowego katalogu
space-add-room-title = Dodaj pokój
accounts-menu = Menu konta
space-add-title = Dodaj przestrzeń
space-address-placeholder = Adres lub zaproszenie do przestrzeni
space-service-label = Usługa Spaces
space-service-placeholder = Usługa Spaces (automatycznie, jeśli puste)
space-name = Nazwa przestrzeni
room-settings-title = Ustawienia pokoju
report-reason-placeholder = Powód (opcjonalnie)
chat-clear-title = Wyczyść historię
rooms-search-public = Szukaj publicznych pokoi
rooms-search-placeholder = Nazwa pokoju lub słowo kluczowe

call-authenticated = Uwierzytelniono
call-reconnecting = Ponowne łączenie

report-title = Zgłoś treść

## Dates and numbers.
date-month-1 = stycznia
date-month-2 = lutego
date-month-3 = marca
date-month-4 = kwietnia
date-month-5 = maja
date-month-6 = czerwca
date-month-7 = lipca
date-month-8 = sierpnia
date-month-9 = września
date-month-10 = października
date-month-11 = listopada
date-month-12 = grudnia
date-medium = { $day } { $month } { $year }
date-time = { $date }, { $time }
time-hm = { $hour }:{ $minute }

profile-listening = Słucha
profile-mood = Nastrój
profile-activity = Aktywność
profile-location = Lokalizacja
profile-idle-since = Bezczynny od
profile-capabilities = Możliwości
profile-subscriptions = Publiczne subskrypcje

## Calls and shell chrome.
call-calling = Dzwonienie…
call-ringing = Dzwoni…
call-active = W trakcie połączenia
call-ended = Zakończono
call-declined = Odrzucono
call-missed = Nieodebrane
call-failed = Niepowodzenie
call-incoming = Przychodzące
call-shared-screen = Udostępniony ekran
call-remote-screen = Zdalny ekran
call-remote-video = Zdalne wideo
call-screen-preview = Podgląd ekranu
call-camera-preview = Podgląd kamery
tray-show = Pokaż Jabu
tray-quit = Zakończ
shell-skip-content = Przejdź do treści
titlebar-minimize = Minimalizuj
titlebar-maximize = Maksymalizuj
titlebar-close = Zamknij
common-yes = Tak
common-no = Nie
search-placeholder = Szukaj wiadomości, kontaktów i pokoi
feed-open-hint = Twój kanał społecznościowy jest otwarty w głównym obszarze.

## Space rail and menus.
a11y-unread = { $count } nieprzeczytanych
a11y-muted = wyciszone
a11y-pending-invite = oczekujące zaproszenie
a11y-locked = zablokowane
space-actions = Działania przestrzeni
space-label = Przestrzeń
space-channel = Kanał
space-mark-read = Oznacz jako przeczytane
space-pin = Przypnij przestrzeń
space-unpin = Odepnij przestrzeń
space-unmute = Wyłącz wyciszenie
space-muted-for = Wyciszone na { $duration }
space-muted-forever = Wyciszone, dopóki nie wyłączysz
space-until-unmute = Dopóki nie włączę z powrotem
space-leave = Opuść przestrzeń
space-leave-confirm = Opuścić przestrzeń?
space-leave-warning = Nie zobaczysz już { $name } na pasku. Wiadomości pozostaną na serwerze.
space-leaving = Opuszczanie…
space-info = Informacje o przestrzeni
space-hide-muted = Ukryj wyciszone kanały
space-invite-menu = Zaproś do przestrzeni…
space-share-link = Udostępnij link przestrzeni
space-share-link-menu = Udostępnij link przestrzeni…
space-create-category-menu = Utwórz kategorię…
space-create-channel-menu = Utwórz kanał…
space-link-room-menu = Połącz istniejący pokój…
space-mute-channel = Wycisz kanał
space-unmute-channel = Wyłącz wyciszenie kanału
space-no-rooms = Brak pokoi
space-all-hidden = Wszystkie kanały są ukryte
space-unhide-hint = Pokaż wyciszone kanały, aby znów je zobaczyć.
space-empty-hint = Pokoje dodane do tej przestrzeni pojawią się tutaj.
time-in-minutes = { $count } min
time-in-hours = { $count } godz.
time-in-days = { $count } dni

## Space and room dialogs.
common-creating = Tworzenie…
common-subscribing = Subskrybowanie…
common-subscribe = Subskrybuj
common-discovering = Odkrywanie…
common-saving = Zapisywanie…
common-optional = Opcjonalne
common-error-jid = Podaj prawidłowy JID.
space-error-category-name = Podaj nazwę kategorii.
space-category-placeholder = np. Rozwój
space-error-managers-only = Tylko administratorzy przestrzeni mogą tworzyć kanały.
space-error-channel-name = Podaj nazwę kanału.
space-error-room-exists = Pokój o tym adresie już istnieje w tej przestrzeni.
space-error-create-room = Nie można utworzyć pokoju: { $error }
space-error-add-room = Pokój został utworzony, ale nie można go dodać do przestrzeni: { $error }
space-channel-placeholder = np. general
space-text-channel = Kanał tekstowy
space-voice-channel = Kanał głosowy
space-no-category = Bez kategorii
space-parent = Nadrzędny
space-linked-nodes = Połączone węzły
space-related-nodes = Powiązane węzły
space-related-hint = Węzły podrzędne lub połączone w tej usłudze
pubsub-invalid-service = nieprawidłowa usługa PubSub
space-no-related = Nie znaleziono powiązanych węzłów
space-hierarchy = Subskrypcja hierarchii
space-hierarchy-hint = Obserwuj ten węzeł, jego dzieci lub całe poddrzewo
space-scope-node = Ten węzeł
space-scope-children = Dzieci
space-scope-all = Wszyscy potomkowie
space-scope-node-lower = ten węzeł
space-scope-children-lower = węzły podrzędne
space-scope-all-lower = wszyscy potomkowie
space-subscribed = Zasubskrybowano { $scope }
space-role-member = Członek
space-role-admin = Administrator
space-role-owner = Właściciel
space-role-outcast = Wykluczony
space-role-publisher = Wydawca
space-role-none = Brak
space-affiliation-none = Brak przynależności
space-loading-members = Ładowanie członków…
space-no-members = Usługa nie zwróciła członków.
members-pending = Oczekuje
space-config-title = Konfiguracja przestrzeni
common-loading-config = Ładowanie konfiguracji…
space-rooms-dirs-title = Pokoje i katalogi przestrzeni
space-directories = Katalogi
space-kind-text = Tekst
space-kind-voice = Głos
space-no-directory = Bez katalogu
space-link-room-title = Połącz istniejący pokój
space-voice-name-placeholder = Nazwa kanału głosowego
space-error-room-address = Podaj adres pokoju.
space-error-room-address-invalid = Podaj prawidłowy adres pokoju.
space-join-existing = Dołącz do istniejącej przestrzeni
space-error-invite = Podaj prawidłowy link zaproszenia do przestrzeni.
space-create-new = Utwórz nową przestrzeń
space-public = Publiczna
space-private = Prywatna
space-error-name = Podaj nazwę przestrzeni.

## Chat labels.
chat-you = Ty
chat-attachment-fallback = [załącznik]
chat-message-yourself = Napisz do siebie
chat-note-to-self = Notatka osobista
chat-back-to-chats = Wróć do czatów
chat-forward-title = Przekaż wiadomość
chat-forward-failed = Nie można przekazać tej wiadomości.
chat-forward-none = Brak innych rozmów do przekazania.
room-error-jid = Nieprawidłowy JID pokoju
room-saved = Konfiguracja pokoju zapisana.
room-destroyed = Pokój zniszczony.
room-config-title = Konfiguracja pokoju
room-error-invite-jid = Podaj prawidłowy JID do zaproszenia
room-invite-title = Zaproś do pokoju
story-yours = Twoja historia

## Space roles shown in member lists.
space-role-none-label = Brak

## Contacts, gateways and rooms.
presence-invisible-error = Ten serwer nie obsługuje trybu niewidocznego (XEP-0186).
calls-automatic = Automatycznie
contact-idle = bezczynny
contact-block = Zablokuj
contact-unblock = Odblokuj
contact-presence-wants = Ten kontakt chce widzieć Twoją obecność.
contact-presence-mutual = Obecność nie jest udostępniana w obie strony.
contact-not-found = Nie znaleziono kontaktu
contact-groups = Grupy (oddzielone przecinkami)
contacts-all = Wszystkie kontakty
profile-name = Imię
profile-role = Rola
profile-no-subscriptions = Brak publicznych subskrypcji
profile-shared-media = Udostępnione media
profile-no-media = Brak udostępnionych mediów
profile-resources = Zasoby
profile-no-presence = Brak informacji o obecności
common-invalid-jid = Nieprawidłowy JID
common-removing = Usuwanie…
common-publishing = Publikowanie…
commands-adhoc-title = Polecenia ad hoc
commands-still-running = Polecenie nadal działa.
commands-loading = Ładowanie poleceń…
commands-none = Brak ogłoszonych poleceń
gateway-none = Ten serwer nie ogłasza bram.
gateway-removed = Rejestracja usunięta.
gateway-submitted = Rejestracja wysłana.
gateway-looking = Szukanie bram…
gateway-registered = To konto jest zarejestrowane.
gateway-loading-form = Ładowanie formularza…
chat-attachment-name = Załącznik
rooms-none = Nie dołączono do żadnego pokoju
room-pin = Przypnij pokój
room-unpin = Odepnij pokój
rooms-error-service-jid = Podaj prawidłowy JID usługi wyszukiwania.
rooms-error-term = Podaj frazę wyszukiwania.
rooms-none-found = Nie znaleziono publicznych pokoi.
rooms-find = Znajdź publiczne pokoje
rooms-not-open = Zamknięty

## Communities and posts.
communities-explore = Odkrywaj społeczności
communities-following = Obserwowane
communities-none-following = Nie obserwujesz jeszcze żadnej społeczności.
communities-cached = W pamięci podręcznej
communities-discover-hint = Odkrywaj publiczne węzły PubSub i obserwuj je.
communities-suggestions = Sugestie
communities-suggested = Sugerowane przez kontakty
communities-none-cached = Brak społeczności w pamięci podręcznej. Wyszukaj serwer PubSub powyżej.
community-error-required = Serwer, węzeł i tytuł są wymagane
community-articles = Artykuły
community-create-action = Utwórz społeczność
community-follow = Obserwuj
community-unfollow = Przestań obserwować
community-settings-title = Ustawienia społeczności
community-policy-open = Każdy może publikować
community-policy-subscribers = Subskrybenci mogą publikować
community-policy-publishers = Tylko wydawcy
community-delete-confirm = Usunąć tę społeczność i wszystkie jej posty?
community-no-affiliation = Nie zwrócono informacji o przynależności.
feed-error-title = Dodaj tytuł posta
feed-new-community-post = Nowy post społeczności
feed-editing = Edytowanie posta
feed-tags-hint = rust, xmpp
feed-none = Brak postów
post-no-comments = Brak komentarzy
post-edited = · edytowano
story-play = Odtwórz
story-pause = Wstrzymaj
story-unavailable = Historia niedostępna
story-unavailable-short = Ta historia nie jest już dostępna.
story-unavailable-detail = Ta historia nie jest już dostępna. Mogła wygasnąć lub nie masz dostępu.
story-delete-confirm = Usunąć tę historię?
story-delete-hint = Zostanie natychmiast wycofana z węzła historii.
story-no-share-targets = Brak kontaktów lub pokoi do udostępnienia.
story-error-image = Nie można odczytać tego obrazu
story-error-media = Historia wymaga obrazu lub wideo
story-error-no-camera = Nie znaleziono kamery
story-error-camera-image = Kamera zwróciła nieczytelny obraz
story-error-crop = Nie można przyciąć tego obrazu
story-new = Nowa historia
story-crop-preview-alt = podgląd przycięcia historii
story-zoom = Powiększenie
story-horizontal = Poziomo
story-vertical = Pionowo
story-camera-preview-alt = podgląd kamery
story-default-camera = Domyślna kamera
story-publish = Opublikuj historię

## Chat message details and OMEMO.
chat-private-notes = Prywatne notatki
room-subject = Temat
room-no-occupants = Brak widocznych uczestników.
room-kicked = Wyrzucony
room-banned = Zbanowany
omemo-bundle-missing = pakiet kluczy nie został pobrany
omemo-trusted = Zaufane
omemo-untrusted = Niezaufane
omemo-unverified = Niezweryfikowane
omemo-signature-invalid = nieprawidłowy podpis
chat-sticker = Naklejka
chat-forwarded = Wiadomość przekazana
chat-mentioned-you = wspomniał o Tobie
chat-attention = Uwaga
chat-edited-suffix = (edytowano)
chat-upload-cancelled = Wysyłanie anulowane
omemo-toggle = Przełącz szyfrowanie OMEMO
omemo-toggle-room = Przełącz szyfrowanie OMEMO pokoju

## Emoji picker and report dialog.
emoji-title = Emoji
emoji-recent = Ostatnie
emoji-favorites = Ulubione
emoji-custom = Niestandardowe
emoji-tone = Odcień
emoji-default-tone = Domyślny
emoji-toggle-favorite = Przełącz :{ $shortcut }: w ulubionych
emoji-tone-light = jasny odcień skóry
emoji-tone-medium-light = jasny średni odcień skóry
emoji-tone-medium = średni odcień skóry
emoji-tone-medium-dark = ciemny średni odcień skóry
emoji-tone-dark = ciemny odcień skóry
emoji-group-smileys = Uśmiechy i emocje
emoji-group-people = Ludzie i ciało
emoji-group-animals = Zwierzęta i natura
emoji-group-food = Jedzenie i napoje
emoji-group-travel = Podróże i miejsca
emoji-group-activities = Aktywności
emoji-group-objects = Przedmioty
emoji-group-symbols = Symbole
emoji-group-flags = Flagi
emoji-group-component = Komponenty
emoji-group-skin-tones = Odcienie skóry
report-target = Cel
report-no-host = Nie skonfigurowano Hostu zgłoszeń. Dodaj [reporting] host = "https://…" do config.toml.
stickers-invalid = Nieprawidłowe dane naklejki
call-connecting = Łączenie…

## Public Host pages.
host-nav-about = Informacje
host-footer-served = Udostępniane przez { $site }
host-footer-about = O tej instancji
host-blog-description = Publiczne posty { $title }
host-blog-empty = Brak publicznych postów.
host-by = Autor
host-post-fallback = Post
host-index-hint = Otwórz publiczny blog za pomocą { $code }.
host-about-title = O { $site }
host-about-body = Ta instancja udostępnia wyłącznie publiczne treści XMPP. Czaty, prywatne posty i dane kont nigdy nie są tu ujawniane.
host-contact-description = Publiczny profil { $title }
host-contact-no-subscriptions = Brak publicznych subskrypcji.
host-public-blog = Publiczny blog
host-public-subscriptions = Publiczne subskrypcje
host-report-page-title = Zgłoś stronę
host-report-description = Zgłoś publiczną stronę tej instancji.
host-target = Cel:
host-report-reason = Powód
host-report-details = Szczegóły
host-report-contact = Twój kontakt (opcjonalnie)
host-report-note = Zgłoszenia są sprawdzane przez moderatorów tej instancji.
host-report-received = Zgłoszenie otrzymane
host-report-thanks = Dziękujemy. Id Twojego zgłoszenia to { $id }.
host-status = Status:
host-report-check = Sprawdź status później
host-report-status-title = Status zgłoszenia
host-report-status-description = Status publicznego zgłoszenia.
host-report-open = Oczekuje na sprawdzenie.
host-report-hidden = Strona została ukryta na tej instancji.
host-report-dismissed = Nie podjęto żadnych działań.
host-report-created = Utworzono { $time }
host-not-found = Nie znaleziono
host-not-found-detail = Ta strona nie istnieje lub jej treść nie jest publiczna.
host-community-description = Publiczna społeczność { $title }
host-community-empty = Brak publicznych artykułów.
host-community-label = Społeczność
host-space-description = Publiczna przestrzeń { $title }
host-space-rooms = Pokoje
host-space-empty = Brak publicznych pokoi.
host-voice = głos
host-story = Historia
host-story-hint = Historia znika po 24 godzinach.
host-share = Udostępnij:
host-open-in-jabu = Otwórz w Jabu
host-report-page = Zgłoś tę stronę
host-article-fallback = Artykuł
host-unavailable = Publiczna treść jest tymczasowo niedostępna.

## Host admin pages.
host-admin-title = Administracja
host-admin-description = Administracja instancji.
host-admin-signin-title = Logowanie administratora
host-admin-token-label = Token administratora
host-admin-signin = Zaloguj się
host-admin-settings = Ustawienia
host-admin-tokens = Tokeny
host-admin-backup = Kopia zapasowa
host-admin-reporter = zgłaszający { $name }
host-admin-action-hide = Ukryj
host-admin-action-unhide = Pokaż
host-admin-action-dismiss = Odrzuć
host-admin-action-reopen = Otwórz ponownie
host-admin-note-placeholder = Notatka (opcjonalnie)
host-admin-apply = Zastosuj
host-admin-queue-title = Kolejka moderacji
host-admin-report-count = { $count } zgłoszeń
host-admin-signout = Wyloguj
host-admin-no-reports = Brak zgłoszeń.
host-admin-audit = Dziennik audytu
host-admin-no-actions = Brak działań.
host-admin-moderation-title = Moderacja
host-admin-moderation-description = Kolejka zgłoszeń i dziennik audytu.
host-admin-settings-title = Ustawienia instancji
host-admin-site-name = Nazwa witryny
host-admin-description-label = Opis
host-admin-accent = Kolor akcentu
host-admin-footer-note = Notatka w stopce
host-admin-footer-placeholder = Opcjonalny tekst w stopce
host-admin-featured = JID-y wyróżnionych blogów, po jednym w wierszu
host-admin-retention = Przechowywanie (dni, 0 przechowuje bezterminowo)
host-admin-save-settings = Zapisz ustawienia
host-admin-back = Wróć do moderacji
host-admin-config-note = Adres nasłuchu, publiczny URL, TTL pamięci podręcznej, limit szybkości i ścieżka bazy danych pozostają w pliku konfiguracyjnym.
host-admin-settings-description = Ustawienia i identyfikacja instancji.
host-admin-new-token = Nowy token (skopiuj go teraz, nie zostanie ponownie pokazany):
host-admin-created = utworzono { $time }
host-admin-revoke = Unieważnij
host-admin-tokens-title = Tokeny administratora
host-admin-create-title = Utwórz token
host-admin-label = Etykieta
host-admin-label-placeholder = np. konsola android
host-admin-role = Rola
host-admin-role-moderator = Moderator
host-admin-role-admin = Administrator
host-admin-create = Utwórz token
host-admin-no-tokens = Brak tokenów.
host-admin-tokens-description = Tokeny i role administratora.
host-admin-invalid-token = Nieprawidłowy token administratora
host-admin-settings-saved = Ustawienia zapisane.
host-admin-token-label-error = Etykiety tokenów muszą mieć od 1 do 60 znaków.
host-admin-unknown-role = Nieznana rola
host-admin-status-open = otwarte
host-admin-status-hidden = ukryte
host-admin-status-dismissed = odrzucone
host-admin-site-required = Nazwa witryny jest wymagana.
host-admin-site-too-long = Nazwa witryny może mieć najwyżej { $count } znaków.
host-admin-description-too-long = Opis może mieć najwyżej { $count } znaków.
host-admin-footer-too-long = Notatka w stopce może mieć najwyżej { $count } znaków.
host-admin-too-many-blogs = Dozwolone są najwyżej { $count } wyróżnione blogi.
host-admin-invalid-jid = `{ $jid }` nie jest prawidłowym JID-em.
host-admin-retention-too-long = Przechowywanie musi wynosić od 0 do { $count } dni.
host-admin-accent-invalid = Kolor akcentu musi wyglądać jak #7dd3fc.

## On-device translation.
settings-translation = Tłumaczenie
settings-translation-privacy = Tłumaczenie działa na tym urządzeniu. Treść wiadomości nigdy nie jest wysyłana na serwer.
settings-translation-enabled = Tłumacz wiadomości
settings-translation-auto = Automatycznie tłumacz przychodzące wiadomości
settings-translation-target = Język tłumaczenia
settings-translation-target-ui = Język interfejsu
settings-translation-model = Model
settings-translation-no-model = Nie zainstalowano modelu
settings-translation-download = Pobierz
settings-translation-downloading = Pobieranie…
settings-translation-installed = Zainstalowano
settings-translation-restart = Uruchom Jabu ponownie, aby użyć nowo zainstalowanego modelu.
chat-translate = Przetłumacz
chat-translating = Tłumaczenie…
chat-translation-failed = Tłumaczenie nie powiodło się
chat-machine-translation = Tłumaczenie maszynowe
chat-show-original = Pokaż oryginał
chat-hide-original = Ukryj oryginał
chat-auto-translate = Automatycznie tłumacz ten czat
settings-translation-notifications = Tłumacz powiadomienia
