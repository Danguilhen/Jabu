## Common actions and labels.
common-close = إغلاق
common-cancel = إلغاء
common-save = حفظ
common-retry = إعادة المحاولة
common-search = بحث
common-send = إرسال رسالة
common-attach = إرفاق ملف
common-remove-attachment = إزالة المرفق
common-loading = جارٍ التحميل…
common-done = تم
common-delete = حذف
common-edit = تعديل

## Navigation and shell sections.
nav-chats = الدردشات
nav-contacts = جهات الاتصال
nav-rooms = الغرف
nav-feed = التحديثات
nav-communities = المجتمعات
nav-sections = الأقسام والمساحات
nav-accounts = الحسابات
nav-add-space = إضافة مساحة
notify-always = كل الرسائل
notify-mentions = الإشارات فقط
notify-never = لا شيء
nav-settings = الإعدادات

## Language settings.
language-title = اللغة
language-system = لغة النظام
language-hint = يُطبَّق على التطبيق بأكمله.

## Counted messages.
chat-typing = يكتب…
chat-unread =
    { $count ->
        [zero] لا رسائل غير مقروءة
        [one] رسالة واحدة غير مقروءة
        [two] رسالتان غير مقروءتين
        [few] { $count } رسائل غير مقروءة
        [many] { $count } رسالة غير مقروءة
       *[other] { $count } رسالة غير مقروءة
    }

space-members = الأعضاء
space-notifications = الإشعارات
space-address = العنوان

room-members =
    { $count ->
        [zero] لا أعضاء
        [one] عضو واحد
        [two] عضوان
        [few] { $count } أعضاء
        [many] { $count } عضوًا
       *[other] { $count } عضو
    }

## Relative time.
time-now = الآن
time-minutes-ago = قبل { $count } دقيقة
time-hours-ago = قبل { $count } ساعة
time-days-ago = قبل { $count } يوم

## Application shell and authentication.
app-client = عميل XMPP أصلي
login-tagline = محادثاتك، بلا ضجيج.
login-description = عميل سطح مكتب سريع وخاص لـ XMPP. دردش، وانضم إلى الغرف، وشارك الملفات، واحتفظ بسجلك على جهازك.
login-direct = الرسائل المباشرة والغرف والأرشيف
login-encrypted = دردشات فردية مشفرة باستخدام OMEMO
login-local = تخزين محلي تحت سيطرتك
login-welcome = مرحبًا بعودتك
login-signin = تسجيل الدخول
login-subtitle = استخدم عنوان XMPP وكلمة المرور.
common-jid = JID
common-password = كلمة المرور
common-resource = المورد
login-insecure = السماح بشهادات TLS غير صالحة (للت debugging فقط)
login-remember = تذكر هذا الحساب
login-connecting = جارٍ الاتصال…
login-password-note = تُرسَل كلمة المرور إلى خادم XMPP فقط.
login-password-placeholder = كلمة المرور
login-error-jid = أدخل JID كاملًا، مثل user@example.com
login-error-password = أدخل كلمة المرور

connection-connecting = جارٍ الاتصال بالخادم…
connection-connecting-detail = في انتظار اتصال XMPP والمصادقة.
connection-failed = تعذر الاتصال
connection-disconnected = غير متصل
connection-disconnected-detail = أنت غير متصل بخادم XMPP.

section-home = الرسائل المباشرة والغرف
section-contacts = الأشخاص في قائمتك
section-rooms = اكتشف الغرف وأدرها
section-feed = المجتمعات والمنشورات
section-communities = تابع المجتمعات واستكشفها

nav-item-unread =
    { $label }, { $count ->
        [zero] لا رسائل غير مقروءة
        [one] رسالة واحدة غير مقروءة
        [two] رسالتان غير مقروءتين
        [few] { $count } رسائل غير مقروءة
        [many] { $count } رسالة غير مقروءة
       *[other] { $count } رسالة غير مقروءة
    }

## Presence and account menus.
presence-title = التواجد
presence-status = الحالة
presence-online = متصل
presence-away = بعيد
presence-idle = خامل
presence-dnd = عدم الإزعاج
presence-xa = بعيد لفترة طويلة
presence-invisible = مخفي
presence-custom = رسالة مخصصة
presence-custom-placeholder = ماذا تفعل؟
presence-priority = الأولوية
presence-mark-idle = وضع علامة خامل
presence-invisible-unsupported = هذا الخادم لا يدعم الوضع المخفي.
presence-invisible-xep = خادمك لا يدعم الوضع المخفي (XEP-0186)
presence-set-custom = تعيين حالة مخصصة…
presence-edit-profile = تعديل الملف الشخصي
presence-manage-accounts = إدارة الحسابات…
presence-gateways = البوابات
presence-sign-out = تسجيل الخروج

## Accounts.
accounts-title = الحسابات
accounts-add = إضافة حساب
accounts-add-connect = إضافة واتصال
accounts-active = نشط
accounts-online = متصل
accounts-offline = غير متصل
accounts-notify = إشعار
accounts-switch = تبديل
accounts-remove = إزالة
accounts-password-missing = أدخل كلمة مرور الحساب.
accounts-not-connected = هذا الحساب غير متصل.\nأعد تشغيل Jabu لبدءه.
accounts-no-config = لا يوجد إعداد محفوظ لهذا الحساب.

## Settings.
settings-title = الإعدادات
settings-theme = السمة
theme-light = فاتح
theme-silver = فضي
theme-dark = داكن
theme-butter = زبدة
settings-font-size = حجم الخط
settings-compact = تباعد مضغوط
settings-autostart = البدء عند تسجيل الدخول
settings-notifications = إشعارات سطح المكتب
settings-microphone = الميكروفون
settings-camera = الكاميرا
settings-sfu = SFU المكالمات
settings-timestamps = إظهار الطوابع الزمنية
settings-link-previews = معاينات الروابط
settings-link-previews-hint = يجلب بيانات الصفحات المرتبطة عند طلب معاينة.
settings-shortcuts = اختصارات لوحة المفاتيح
settings-shortcut-search = بحث
settings-shortcut-settings = الإعدادات
settings-shortcut-sections = تبديل القسم
settings-shortcut-focus = التركيز على المحادثة / الشريط الجانبي
settings-shortcut-close = إغلاق مربع حوار أو طبقة
settings-shortcut-send = إرسال رسالة
settings-shortcut-menu = فتح قائمة سياقية

## Conversations.
chat-no-conversations = لا توجد محادثات بعد
chat-no-archived = لا توجد محادثات مؤرشفة
chat-unread-filter = غير مقروء
chat-search-placeholder = البحث في المحادثات
chat-no-messages = لا توجد رسائل بعد
chat-load-older = تحميل رسائل أقدم
chat-view-story = عرض القصة
chat-pin = تثبيت
chat-unpin = إلغاء التثبيت
chat-mute-action = كتم
chat-unmute = إلغاء الكتم
chat-archive = أرشفة
chat-unarchive = إلغاء الأرشفة
chat-forward = إعادة توجيه
chat-react = تفاعل { $emoji }
chat-saved-to = حُفظ في { $path }
chat-unread-messages = { $count } رسالة غير مقروءة
chat-empty-state = اختر دردشة أو غرفة من الشريط الجانبي، أو ابحث في رسائلك وجهات اتصالك.
chat-pinned = مثبت
chat-muted = مكتوم
chat-online = متصل
chat-away = بعيد
chat-dnd = عدم الإزعاج
chat-xa = بعيد لفترة طويلة
chat-offline = غير متصل
chat-audio-call = بدء مكالمة صوتية
chat-video-call = بدء مكالمة فيديو
chat-search-messages = البحث في الرسائل
chat-clear-history = مسح سجل هذه المحادثة
chat-change-subject = تغيير الموضوع
chat-nudge = إرسال طلب انتباه
chat-room-subject = موضوع الغرفة
chat-cancel-reply = إلغاء الرد
chat-back = رجوع
chat-sending = جارٍ الإرسال…
chat-failed = فشل
chat-edited = معدّل
chat-encrypted = مشفر
chat-reply = رد
chat-reply-thread = رد في السلسلة
chat-in-thread = في سلسلة
chat-add-reaction = إضافة تفاعل
chat-delete = حذف
chat-delete-everyone = حذف للجميع
chat-download = تنزيل إلى ذاكرة المرفقات المؤقتة
chat-message = رسالة
chat-send-placeholder = أرسل رسالة…
chat-attach = إرفاق ملف
chat-send = إرسال رسالة
chat-send-sticker = إرسال ملصق
chat-insert-emoji = إدراج رمز تعبيري
chat-stop-upload = إيقاف رفع الملفات المتبقية
chat-uploading = جارٍ الرفع…
chat-attach-short = إرفاق
chat-searching = جارٍ البحث…
chat-type-more = اكتب حرفين على الأقل.
chat-no-results = لا نتائج.
chat-results-messages = الرسائل
chat-results-rooms = رسائل الغرف
chat-results-contacts = جهات الاتصال
chat-results-rooms-list = الغرف
chat-results-posts = المنشورات
chat-results-spaces = المساحات
chat-incoming-audio = مكالمة صوتية واردة
chat-incoming-video = مكالمة فيديو واردة
chat-incoming-call = مكالمة واردة
chat-decline = رفض
chat-accept = قبول
chat-voice-channel = قناة صوتية
chat-leave = مغادرة
chat-join = انضمام
chat-conn-screen = جارٍ الاتصال بشاشة الطرف الآخر…
chat-starting-screen = جارٍ بدء مشاركة الشاشة…
chat-starting-camera = جارٍ تشغيل الكاميرا…
chat-call-progress = مكالمة جارية
chat-camera-on = تشغيل الكاميرا
chat-camera-off = إيقاف الكاميرا
chat-camera-pending = جارٍ تشغيل الكاميرا…
chat-switch-camera = تبديل الكاميرا
chat-share-screen = مشاركة الشاشة
chat-stop-sharing = إيقاف مشاركة الشاشة
chat-hang-up = إنهاء المكالمة
chat-voice-connected = الصوت متصل · { $count } في القناة
chat-join-voice = انضم إلى القناة الصوتية. يُمرَّر الصوت عبر SFU الغرفة.
chat-join-voice-action = الانضمام إلى الصوت
chat-connecting = جارٍ الاتصال…
chat-mute-speaker = كتم مكبر الصوت
chat-unmute-speaker = إلغاء كتم مكبر الصوت
chat-incoming-from =
    { $kind ->
        [audio] مكالمة صوتية واردة من { $name }
        [video] مكالمة فيديو واردة من { $name }
       *[other] مكالمة واردة من { $name }
    }
chat-incoming-kind =
    { $kind ->
        [audio] مكالمة صوتية واردة
        [video] مكالمة فيديو واردة
       *[other] مكالمة واردة
    }
notify-new-story = قصة جديدة
notify-new-post = منشور جديد
notify-story-title = 📝 قصة جديدة من { $author }
mute-15-minutes = 15 دقيقة
mute-1-hour = ساعة واحدة
mute-8-hours = 8 ساعات
mute-24-hours = 24 ساعة

## Space, social and picker surfaces.
space-category-name = اسم الفئة
space-channel-name = اسم القناة
space-channel-type = نوع القناة
space-category = الفئة
space-change-avatar = تغيير الصورة الرمزية
space-change-banner = تغيير اللافتة
space-invite = دعوة
space-configure = تكوين
space-members-todo = إدارة الأعضاء والتكوين غير متصلين بعد.
space-invite-link = رابط الدعوة
space-affiliation = الانتماء
space-invite-help = يمكن لأي شخص لديه هذا الرابط الانضمام إلى المساحة. يمكن لمديري المساحة فقط منح الانتماء.
space-send-invite = إرسال الدعوة
space-approve = موافقة
space-deny = رفض
space-save-config = حفظ التكوين
common-add = إضافة
space-save-directories = حفظ الأدلة
space-link-room = ربط غرفة
space-room-type = نوع الغرفة
space-room-address = عنوان الغرفة
space-display-name = الاسم المعروض
space-join-auto = الانضمام تلقائيًا
space-add-to = إضافة إلى المساحة
space-visibility = الرؤية
space-service-invalid = عنوان خدمة Spaces غير صالح.
common-create = إنشاء
profile-full-name = الاسم الكامل
profile-nickname = اللقب
profile-organisation = المؤسسة
profile-title = المسمى الوظيفي
profile-email = البريد الإلكتروني
profile-phone = الهاتف
profile-website = الموقع الإلكتروني
profile-note = ملاحظة
profile-avatar = الصورة الرمزية
space-confirm-destroy = تأكيد التدمير
space-destroy = تدمير
chat-clear-help = يؤدي هذا إلى إزالة الرسائل من هذا الجهاز والتخزين المحلي. لا تُحذف الرسائل الموجودة على الخادم.
chat-clear = مسح
commands-title = الأوامر
common-next = التالي
common-finish = إنهاء
common-submit = إرسال
contact-open-chat = فتح الدردشة
common-refresh = تحديث
report-action = إبلاغ
contact-presence-sent = أُرسل طلب التواجد. في انتظار الموافقة.
contact-request-presence = طلب التواجد
gateway-register = تسجيل
contacts-requests = طلبات جهات الاتصال
contacts-none = لا توجد جهات اتصال
contacts-awaiting = في انتظار الموافقة
contact-info = معلومات
gateway-search-service = البحث عن خدمة
common-load-more = تحميل المزيد
common-send-action = إرسال
common-new = جديد
feed-publish = نشر منشور
feed-new-post = منشور جديد
community-no-posts = لا توجد منشورات في هذا المجتمع بعد.
community-save = حفظ الإعدادات
community-delete = حذف المجتمع
feed-tags-placeholder = الوسوم (مفصولة بفواصل)
feed-load-older = تحميل منشورات أقدم
story-share = مشاركة
story-open-media = فتح وسائط القصة
story-encrypt-comment = تشفير التعليق باستخدام OMEMO
chat-encrypt = التشفير باستخدام OMEMO
story-retake = إعادة الالتقاط
story-replace = استبدال
story-take-photo = التقاط صورة
story-cancel-camera = إلغاء الكاميرا
story-choose = اختر صورة أو فيديو أو التقط صورة
story-gallery = المعرض
members-kick = طرد
members-ban = حظر
omemo-hide = إخفاء أجهزة التشفير
omemo-devices = أجهزة التشفير
omemo-fetch = جلب المفاتيح
omemo-heartbeat = نبضة
omemo-no-jids = هذه الغرفة لا تعرض JID حقيقية أو ليس لديها أعضاء معروفون.
omemo-none = لا توجد أجهزة OMEMO معروفة بعد. فعّل التشفير في المحرر لجلبها.
omemo-trust = الثقة
omemo-distrust = عدم الثقة
sticker-load = تحميل ملصق
emoji-manage = إدارة
emoji-help = اضغط على رمز تعبيري لإضافته إلى المفضلة أو إزالته منها.
emoji-remove-pack = إزالة الحزمة
emoji-no-match = لا يوجد رمز تعبيري مطابق.
emoji-no-favorites = لا توجد مفضلات بعد. اضغط على المفضلة لإضافة بعضها.
emoji-none = لا توجد رموز تعبيرية هنا بعد.

## Reporting and stickers.
report-reason = السبب
report-spam = بريد مزعج
report-abuse = إساءة أو مضايقة
report-illegal = محتوى غير قانوني
report-other = أخرى
report-details = التفاصيل (اختياري)
report-send = إرسال البلاغ
report-sending = جارٍ الإرسال…
report-stored = حُفظ البلاغ. المعرّف { $id }، الحالة { $status }.
stickers-title = الملصقات
stickers-import-hint = استيراد حزمة ملصقات (.zip)
stickers-import-action = استيراد
stickers-no-packs = لا توجد حزم ملصقات بعد.
stickers-add-hint = أضف ملف ZIP يحتوي على صور أو حزمة ملصقات Movim مع meta.json.
stickers-import-pack = استيراد حزمة الملصقات

## Remaining UI labels.
space-invitee-jid = JID المدعو
profile-menu-title = الملف الشخصي
contact-jid = JID جهة الاتصال
contact-nickname-placeholder = اللقب (اختياري)
rooms-search-title = البحث عن غرف
rooms-join = الانضمام إلى الغرفة
post-like = إعجاب
post-comment = تعليق
post-comment-placeholder = اكتب تعليقًا…
community-pubsub = خدمة PubSub
community-create = إنشاء مجتمع
community-node = معرّف العقدة
community-title = عنوان المجتمع
feed-post = نشر
feed-post-title = عنوان المنشور
feed-post-body = نص المنشور
feed-post-placeholder = اكتب شيئًا…
feed-tags = الوسوم
community-label = المجتمع
community-description = الوصف
community-members = أعضاء المجتمع
report-this-post = الإبلاغ عن هذا المنشور
feed-edit = تعديل المنشور
feed-delete = حذف المنشور
story-title = قصة
story-comment-placeholder = إرسال تعليق…
story-delete = حذف هذه القصة
story-share-title = مشاركة القصة
story-caption = التسمية التوضيحية
story-caption-placeholder = إضافة تسمية توضيحية…
main-no-conversation = لم تُحدَّد محادثة
main-conversation = محادثة
chat-nudge-action = تنبيه
chat-preview-link = معاينة الرابط
emoji-favorite = إضافة أو إزالة رمز تعبيري مفضل
emoji-import = استيراد حزمة رموز تعبيرية من Movim (.zip)
common-import = استيراد
emoji-search = البحث عن رمز تعبيري
community-pubsub-hint = خدمة PubSub، مثل pubsub.example.org
community-node-hint = معرّف العقدة، مثل rust
emoji-alias-hint = الاسم المستعار المستخدم في الرسائل، مثل :party:

feed-filter-all = الكل
feed-post-hint = شارك شيئًا مع جهات اتصالك…

## Dialog labels.
accounts-account-jid = JID الحساب
call-connected = متصل
space-menu = قائمة المساحة
channel-actions = إجراءات القناة
space-create-category = إنشاء فئة
space-create-channel = إنشاء قناة
space-members-tab = الأعضاء
space-invite-title = دعوة إلى المساحة
space-members-title = أعضاء المساحة
space-settings-title = إعدادات المساحة
space-rooms-title = غرف المساحة
space-directory-placeholder = اسم الدليل الجديد
space-add-room-title = إضافة غرفة
accounts-menu = قائمة الحساب
space-add-title = إضافة مساحة
space-address-placeholder = عنوان المساحة أو الدعوة
space-service-label = خدمة Spaces
space-service-placeholder = خدمة Spaces (تلقائية إذا كانت فارغة)
space-name = اسم المساحة
room-settings-title = إعدادات الغرفة
report-reason-placeholder = السبب (اختياري)
chat-clear-title = مسح السجل
rooms-search-public = البحث عن غرف عامة
rooms-search-placeholder = اسم الغرفة أو كلمة مفتاحية

call-authenticated = تمت المصادقة
call-reconnecting = جارٍ إعادة الاتصال

report-title = الإبلاغ عن محتوى

## Dates and numbers.
date-month-1 = يناير
date-month-2 = فبراير
date-month-3 = مارس
date-month-4 = أبريل
date-month-5 = مايو
date-month-6 = يونيو
date-month-7 = يوليو
date-month-8 = أغسطس
date-month-9 = سبتمبر
date-month-10 = أكتوبر
date-month-11 = نوفمبر
date-month-12 = ديسمبر
date-medium = { $day } { $month } { $year }
date-time = { $date }، { $time }
time-hm = { $hour }:{ $minute }

profile-listening = يستمع إلى
profile-mood = المزاج
profile-activity = النشاط
profile-location = الموقع
profile-idle-since = خامل منذ
profile-capabilities = الإمكانات
profile-subscriptions = الاشتراكات العامة

## Calls and shell chrome.
call-calling = جارٍ الاتصال…
call-ringing = يرن…
call-active = في مكالمة
call-ended = انتهت
call-declined = مرفوضة
call-missed = فائتة
call-failed = فشلت
call-incoming = واردة
call-shared-screen = الشاشة المشتركة
call-remote-screen = الشاشة البعيدة
call-remote-video = الفيديو البعيد
call-screen-preview = معاينة الشاشة
call-camera-preview = معاينة الكاميرا
tray-show = إظهار Jabu
tray-quit = إنهاء
shell-skip-content = الانتقال إلى المحتوى
titlebar-minimize = تصغير
titlebar-maximize = تكبير
titlebar-close = إغلاق
common-yes = نعم
common-no = لا
search-placeholder = البحث في الرسائل وجهات الاتصال والغرف
feed-open-hint = تحديثاتك الاجتماعية مفتوحة في المنطقة الرئيسية.

## Space rail and menus.
a11y-unread = { $count } غير مقروء
a11y-muted = مكتوم
a11y-pending-invite = دعوة معلقة
a11y-locked = مقفل
space-actions = إجراءات المساحة
space-label = مساحة
space-channel = قناة
space-mark-read = وضع علامة مقروء
space-pin = تثبيت المساحة
space-unpin = إلغاء تثبيت المساحة
space-unmute = إلغاء الكتم
space-muted-for = مكتوم لمدة { $duration }
space-muted-forever = مكتوم حتى تلغي الكتم
space-until-unmute = حتى أعيد تشغيله
space-leave = مغادرة المساحة
space-leave-confirm = مغادرة المساحة؟
space-leave-warning = لن ترى { $name } في شريطك بعد الآن. تبقى الرسائل على الخادم.
space-leaving = جارٍ المغادرة…
space-info = معلومات المساحة
space-hide-muted = إخفاء القنوات المكتومة
space-invite-menu = دعوة إلى المساحة…
space-share-link = مشاركة رابط المساحة
space-share-link-menu = مشاركة رابط المساحة…
space-create-category-menu = إنشاء فئة…
space-create-channel-menu = إنشاء قناة…
space-link-room-menu = ربط غرفة موجودة…
space-mute-channel = كتم القناة
space-unmute-channel = إلغاء كتم القناة
space-no-rooms = لا توجد غرف بعد
space-all-hidden = جميع القنوات مخفية
space-unhide-hint = أظهر القنوات المكتومة لرؤيتها مرة أخرى.
space-empty-hint = ستظهر الغرف المضافة إلى هذه المساحة هنا.
time-in-minutes = { $count } دقيقة
time-in-hours = { $count } ساعة
time-in-days = { $count } يوم

## Space and room dialogs.
common-creating = جارٍ الإنشاء…
common-subscribing = جارٍ الاشتراك…
common-subscribe = اشتراك
common-discovering = جارٍ الاكتشاف…
common-saving = جارٍ الحفظ…
common-optional = اختياري
common-error-jid = أدخل JID صالحًا.
space-error-category-name = أدخل اسم الفئة.
space-category-placeholder = مثل التطوير
space-error-managers-only = يمكن لمديري المساحة فقط إنشاء القنوات.
space-error-channel-name = أدخل اسم القناة.
space-error-room-exists = توجد غرفة بهذا العنوان في هذه المساحة بالفعل.
space-error-create-room = تعذر إنشاء الغرفة: { $error }
space-error-add-room = أُنشئت الغرفة، ولكن تعذر إضافتها إلى المساحة: { $error }
space-channel-placeholder = مثل general
space-text-channel = قناة نصية
space-voice-channel = قناة صوتية
space-no-category = بلا فئة
space-parent = الأصل
space-linked-nodes = العقد المرتبطة
space-related-nodes = العقد ذات الصلة
space-related-hint = عقد فرعية أو مرتبطة على هذه الخدمة
pubsub-invalid-service = خدمة PubSub غير صالحة
space-no-related = لم يتم العثور على عقد ذات صلة
space-hierarchy = اشتراك هرمي
space-hierarchy-hint = متابعة هذه العقدة أو أبنائها أو الشجرة الفرعية بأكملها
space-scope-node = هذه العقدة
space-scope-children = الأبناء
space-scope-all = جميع الأحفاد
space-scope-node-lower = هذه العقدة
space-scope-children-lower = العقد الفرعية
space-scope-all-lower = جميع الأحفاد
space-subscribed = مشترك في { $scope }
space-role-member = عضو
space-role-admin = مشرف
space-role-owner = مالك
space-role-outcast = محظور
space-role-publisher = ناشر
space-role-none = لا شيء
space-affiliation-none = بلا انتماء
space-loading-members = جارٍ تحميل الأعضاء…
space-no-members = لم تُرجع الخدمة أي أعضاء.
members-pending = معلق
space-config-title = تكوين المساحة
common-loading-config = جارٍ تحميل التكوين…
space-rooms-dirs-title = غرف المساحة وأدلتها
space-directories = الأدلة
space-kind-text = نص
space-kind-voice = صوت
space-no-directory = بلا دليل
space-link-room-title = ربط غرفة موجودة
space-voice-name-placeholder = اسم القناة الصوتية
space-error-room-address = أدخل عنوان غرفة.
space-error-room-address-invalid = أدخل عنوان غرفة صالحًا.
space-join-existing = الانضمام إلى مساحة موجودة
space-error-invite = أدخل رابط دعوة صالحًا.
space-create-new = إنشاء مساحة جديدة
space-public = عام
space-private = خاص
space-error-name = أدخل اسم المساحة.

## Chat labels.
chat-you = أنت
chat-attachment-fallback = [مرفق]
chat-message-yourself = مراسلة نفسك
chat-note-to-self = ملاحظة شخصية
chat-back-to-chats = العودة إلى الدردشات
chat-forward-title = إعادة توجيه الرسالة
chat-forward-failed = تعذر إعادة توجيه هذه الرسالة.
chat-forward-none = لا توجد محادثات أخرى لإعادة التوجيه إليها.
room-error-jid = JID الغرفة غير صالح
room-saved = حُفظ تكوين الغرفة.
room-destroyed = دُمّرت الغرفة.
room-config-title = تكوين الغرفة
room-error-invite-jid = أدخل JID صالحًا لدعوته
room-invite-title = دعوة إلى الغرفة
story-yours = قصتك

## Space roles shown in member lists.
space-role-none-label = لا شيء

## Contacts, gateways and rooms.
presence-invisible-error = هذا الخادم لا يدعم الوضع المخفي (XEP-0186).
calls-automatic = تلقائي
contact-idle = خامل
contact-block = حظر
contact-unblock = إلغاء الحظر
contact-presence-wants = هذه الجهة تريد رؤية تواجدك.
contact-presence-mutual = التواجد غير مشترك في الاتجاهين.
contact-not-found = لم يتم العثور على جهة الاتصال
contact-groups = المجموعات (مفصولة بفواصل)
contacts-all = كل جهات الاتصال
profile-name = الاسم
profile-role = الدور
profile-no-subscriptions = لا اشتراكات عامة
profile-shared-media = الوسائط المشتركة
profile-no-media = لا وسائط مشتركة بعد
profile-resources = الموارد
profile-no-presence = لا معلومات تواجد
common-invalid-jid = JID غير صالح
common-removing = جارٍ الإزالة…
common-publishing = جارٍ النشر…
commands-adhoc-title = أوامر مخصصة
commands-still-running = لا يزال الأمر قيد التشغيل.
commands-loading = جارٍ تحميل الأوامر…
commands-none = لا توجد أوامر معلنة
gateway-none = هذا الخادم لا يعلن أي بوابات.
gateway-removed = أُزيل التسجيل.
gateway-submitted = أُرسل التسجيل.
gateway-looking = جارٍ البحث عن البوابات…
gateway-registered = هذا الحساب مسجل.
gateway-loading-form = جارٍ تحميل النموذج…
chat-attachment-name = مرفق
rooms-none = لم تنضم إلى أي غرفة
room-pin = تثبيت الغرفة
room-unpin = إلغاء تثبيت الغرفة
rooms-error-service-jid = أدخل JID خدمة بحث صالحًا.
rooms-error-term = أدخل كلمة بحث.
rooms-none-found = لم يتم العثور على غرف عامة.
rooms-find = البحث عن غرف عامة
rooms-not-open = غير مفتوحة

## Communities and posts.
communities-explore = استكشاف المجتمعات
communities-following = المتابَعة
communities-none-following = لا تتابع أي مجتمع بعد.
communities-cached = مخزّن
communities-discover-hint = اكتشف عقد PubSub عامة وتابعها.
communities-suggestions = اقتراحات
communities-suggested = مقترح من جهات الاتصال
communities-none-cached = لا مجتمعات مخزنة بعد. ابحث عن خادم PubSub أعلاه.
community-error-required = الخادم والعقدة والعنوان مطلوبة
community-articles = المقالات
community-create-action = إنشاء مجتمع
community-follow = متابعة
community-unfollow = إلغاء المتابعة
community-settings-title = إعدادات المجتمع
community-policy-open = يمكن للجميع النشر
community-policy-subscribers = يمكن للمشتركين النشر
community-policy-publishers = الناشرون فقط
community-delete-confirm = حذف هذا المجتمع وكل منشوراته؟
community-no-affiliation = لم تُرجع أي معلومات انتماء.
feed-error-title = أضف عنوانًا للمنشور
feed-new-community-post = منشور مجتمع جديد
feed-editing = تعديل المنشور
feed-tags-hint = rust, xmpp
feed-none = لا منشورات بعد
post-no-comments = لا تعليقات بعد
post-edited = · معدّل
story-play = تشغيل
story-pause = إيقاف مؤقت
story-unavailable = القصة غير متاحة
story-unavailable-short = هذه القصة لم تعد متاحة.
story-unavailable-detail = هذه القصة لم تعد متاحة. ربما انتهت صلاحيتها أو ليس لديك إذن بالوصول.
story-delete-confirm = حذف هذه القصة؟
story-delete-hint = ستُسحب فورًا من عقدة قصصك.
story-no-share-targets = لا توجد جهات اتصال أو غرف للمشاركة معها.
story-error-image = تعذر قراءة تلك الصورة
story-error-media = تحتاج القصة إلى صورة أو فيديو
story-error-no-camera = لم يتم العثور على كاميرا
story-error-camera-image = أعادت الكاميرا صورة غير قابلة للقراءة
story-error-crop = تعذر قص تلك الصورة
story-new = قصة جديدة
story-crop-preview-alt = معاينة قص القصة
story-zoom = تكبير
story-horizontal = أفقي
story-vertical = رأسي
story-camera-preview-alt = معاينة الكاميرا
story-default-camera = الكاميرا الافتراضية
story-publish = نشر القصة

## Chat message details and OMEMO.
chat-private-notes = ملاحظات خاصة
room-subject = الموضوع
room-no-occupants = لا يوجد مشاركون ظاهرون بعد.
room-kicked = مطروح
room-banned = محظور
omemo-bundle-missing = حزمة المفاتيح غير منزلة
omemo-trusted = موثوق
omemo-untrusted = غير موثوق
omemo-unverified = غير مُتحقَّق
omemo-signature-invalid = التوقيع غير صالح
chat-sticker = ملصق
chat-forwarded = رسالة معاد توجيهها
chat-mentioned-you = أشار إليك
chat-attention = انتباه
chat-edited-suffix = (معدّل)
chat-upload-cancelled = أُلغي الرفع
omemo-toggle = تبديل تشفير OMEMO
omemo-toggle-room = تبديل تشفير OMEMO للغرفة

## Emoji picker and report dialog.
emoji-title = رموز تعبيرية
emoji-recent = الأخيرة
emoji-favorites = المفضلة
emoji-custom = مخصصة
emoji-tone = اللون
emoji-default-tone = افتراضي
emoji-toggle-favorite = تبديل :{ $shortcut }: في المفضلة
emoji-tone-light = لون بشرة فاتح
emoji-tone-medium-light = لون بشرة فاتح متوسط
emoji-tone-medium = لون بشرة متوسط
emoji-tone-medium-dark = لون بشرة داكن متوسط
emoji-tone-dark = لون بشرة داكن
emoji-group-smileys = الوجوه والمشاعر
emoji-group-people = الأشخاص والجسد
emoji-group-animals = الحيوانات والطبيعة
emoji-group-food = الطعام والشراب
emoji-group-travel = السفر والأماكن
emoji-group-activities = الأنشطة
emoji-group-objects = الأشياء
emoji-group-symbols = الرموز
emoji-group-flags = الأعلام
emoji-group-component = المكوّنات
emoji-group-skin-tones = ألوان البشرة
report-target = الهدف
report-no-host = لم يُكوَّن Host للإبلاغ. أضف [reporting] host = "https://…" إلى config.toml.
stickers-invalid = بيانات الملصق غير صالحة
call-connecting = جارٍ الاتصال…

## Public Host pages.
host-nav-about = حول
host-footer-served = يُقدَّم بواسطة { $site }
host-footer-about = حول هذا الموقع
host-blog-description = منشورات { $title } العامة
host-blog-empty = لا منشورات عامة بعد.
host-by = بواسطة
host-post-fallback = منشور
host-index-hint = افتح مدونة عامة باستخدام { $code }.
host-about-title = حول { $site }
host-about-body = يقدّم هذا الموقع محتوى XMPP عامًا فقط. لا تُعرَض الدردشات أو المنشورات الخاصة أو بيانات الحسابات هنا أبدًا.
host-contact-description = الملف العام لـ { $title }
host-contact-no-subscriptions = لا اشتراكات عامة.
host-public-blog = المدونة العامة
host-public-subscriptions = الاشتراكات العامة
host-report-page-title = الإبلاغ عن صفحة
host-report-description = الإبلاغ عن صفحة عامة إلى هذا الموقع.
host-target = الهدف:
host-report-reason = السبب
host-report-details = التفاصيل
host-report-contact = وسيلة اتصالك (اختياري)
host-report-note = يراجع البلاغات مشرفو هذا الموقع.
host-report-received = تم استلام البلاغ
host-report-thanks = شكرًا. معرّف بلاغك هو { $id }.
host-status = الحالة:
host-report-check = تحقق من الحالة لاحقًا
host-report-status-title = حالة البلاغ
host-report-status-description = حالة بلاغ عام.
host-report-open = في انتظار المراجعة.
host-report-hidden = أُخفيت الصفحة على هذا الموقع.
host-report-dismissed = لم يُتخذ أي إجراء.
host-report-created = أُنشئ { $time }
host-not-found = غير موجود
host-not-found-detail = هذه الصفحة غير موجودة أو محتواها ليس عامًا.
host-community-description = مجتمع عام { $title }
host-community-empty = لا مقالات عامة بعد.
host-community-label = المجتمع
host-space-description = مساحة عامة { $title }
host-space-rooms = الغرف
host-space-empty = لا غرف عامة مدرجة.
host-voice = صوت
host-story = قصة
host-story-hint = تختفي القصة بعد 24 ساعة.
host-share = مشاركة:
host-open-in-jabu = فتح في Jabu
host-report-page = الإبلاغ عن هذه الصفحة
host-article-fallback = مقال
host-unavailable = المحتوى العام غير متاح مؤقتًا.

## Host admin pages.
host-admin-title = الإدارة
host-admin-description = إدارة الموقع.
host-admin-signin-title = تسجيل دخول المشرف
host-admin-token-label = رمز المشرف
host-admin-signin = تسجيل الدخول
host-admin-settings = الإعدادات
host-admin-tokens = الرموز
host-admin-backup = نسخ احتياطي
host-admin-reporter = المُبلِّغ { $name }
host-admin-action-hide = إخفاء
host-admin-action-unhide = إظهار
host-admin-action-dismiss = رفض
host-admin-action-reopen = إعادة فتح
host-admin-note-placeholder = ملاحظة (اختياري)
host-admin-apply = تطبيق
host-admin-queue-title = قائمة الإشراف
host-admin-report-count = { $count } بلاغ
host-admin-signout = تسجيل الخروج
host-admin-no-reports = لا بلاغات.
host-admin-audit = سجل التدقيق
host-admin-no-actions = لا إجراءات بعد.
host-admin-moderation-title = الإشراف
host-admin-moderation-description = قائمة البلاغات وسجل التدقيق.
host-admin-settings-title = إعدادات الموقع
host-admin-site-name = اسم الموقع
host-admin-description-label = الوصف
host-admin-accent = لون التمييز
host-admin-footer-note = ملاحظة التذييل
host-admin-footer-placeholder = نص اختياري يظهر في التذييل
host-admin-featured = JID المدونات المميزة، واحد في كل سطر
host-admin-retention = مدة الاحتفاظ (أيام، 0 يحتفظ دائمًا)
host-admin-save-settings = حفظ الإعدادات
host-admin-back = العودة إلى الإشراف
host-admin-config-note = يبقى عنوان الاستماع وعنوان URL العام وTTL الذاكرة المؤقتة وحد المعدل ومسار قاعدة البيانات في ملف التكوين.
host-admin-settings-description = إعدادات الموقع وهويتها.
host-admin-new-token = رمز جديد (انسخه الآن، لن يُعرض مرة أخرى):
host-admin-created = أُنشئ { $time }
host-admin-revoke = إبطال
host-admin-tokens-title = رموز المشرف
host-admin-create-title = إنشاء رمز
host-admin-label = التسمية
host-admin-label-placeholder = مثل وحدة تحكم Android
host-admin-role = الدور
host-admin-role-moderator = مشرف
host-admin-role-admin = مدير
host-admin-create = إنشاء رمز
host-admin-no-tokens = لا رموز.
host-admin-tokens-description = رموز وأدوار المشرف.
host-admin-invalid-token = رمز المشرف غير صالح
host-admin-settings-saved = حُفظت الإعدادات.
host-admin-token-label-error = يجب أن تكون تسمية الرمز من 1 إلى 60 حرفًا.
host-admin-unknown-role = دور غير معروف
host-admin-status-open = مفتوح
host-admin-status-hidden = مخفي
host-admin-status-dismissed = مرفوض
host-admin-site-required = اسم الموقع مطلوب.
host-admin-site-too-long = يجب ألا يزيد اسم الموقع عن { $count } حرفًا.
host-admin-description-too-long = يجب ألا يزيد الوصف عن { $count } حرفًا.
host-admin-footer-too-long = يجب ألا تزيد ملاحظة التذييل عن { $count } حرفًا.
host-admin-too-many-blogs = يُسمح بـ { $count } مدونة مميزة كحد أقصى.
host-admin-invalid-jid = `{ $jid }` ليس JID صالحًا.
host-admin-retention-too-long = يجب أن تكون مدة الاحتفاظ بين 0 و { $count } يومًا.
host-admin-accent-invalid = يجب أن يشبه لون التمييز #7dd3fc.

## On-device translation.
settings-translation = الترجمة
settings-translation-privacy = تعمل الترجمة على هذا الجهاز. لا يُرسل نص الرسائل إلى أي خادم أبدًا.
settings-translation-enabled = ترجمة الرسائل
settings-translation-auto = ترجمة الرسائل الواردة تلقائيًا
settings-translation-target = لغة الترجمة
settings-translation-target-ui = لغة الواجهة
settings-translation-model = النموذج
settings-translation-no-model = لا يوجد نموذج مثبت
settings-translation-download = تنزيل
settings-translation-downloading = جارٍ التنزيل…
settings-translation-installed = مثبت
settings-translation-restart = أعد تشغيل Jabu لاستخدام النموذج المثبت حديثًا.
chat-translate = ترجمة
chat-translating = جارٍ الترجمة…
chat-translation-failed = فشلت الترجمة
chat-machine-translation = ترجمة آلية
chat-show-original = إظهار الأصل
chat-hide-original = إخفاء الأصل
chat-auto-translate = ترجمة هذه المحادثة تلقائيًا
settings-translation-notifications = ترجمة الإشعارات

## Reposts and threading.
feed-repost = إعادة نشر
feed-in-reply-to = ردًا على منشور
