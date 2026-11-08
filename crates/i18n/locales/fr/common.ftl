## Actions et libellés courants.
common-close = Fermer
common-cancel = Annuler
common-save = Enregistrer
common-retry = Réessayer
common-search = Rechercher
common-send = Envoyer le message
common-attach = Joindre un fichier
common-remove-attachment = Retirer la pièce jointe
common-loading = Chargement…
common-done = Terminé
common-delete = Supprimer
common-edit = Modifier

## Navigation et sections.
nav-chats = Discussions
nav-contacts = Contacts
nav-rooms = Salons
nav-feed = Fil
nav-communities = Communautés
nav-sections = Sections et espaces
nav-accounts = Comptes
nav-add-space = Ajouter un espace
notify-always = Tous les messages
notify-mentions = Mentions uniquement
notify-never = Rien
nav-settings = Paramètres

## Réglages de langue.
language-title = Langue
language-system = Langue du système
language-hint = S'applique à toute l'application.

## Messages comptés.
chat-typing = en train d’écrire…
chat-unread =
    { $count ->
        [one] { $count } message non lu
       *[other] { $count } messages non lus
    }

space-members = Membres
space-notifications = Notifications
space-address = Adresse

room-members =
    { $count ->
        [one] { $count } membre
       *[other] { $count } membres
    }

## Temps relatif.
time-now = à l'instant
time-minutes-ago = il y a { $count } min
time-hours-ago = il y a { $count } h
time-days-ago = il y a { $count } j

## Application shell and authentication.
app-client = Un client XMPP natif
login-tagline = Vos conversations, sans le bruit.
login-description = Un client de bureau rapide et privé pour XMPP. Discutez, rejoignez des salons, partagez des fichiers et gardez votre historique sur votre appareil.
login-direct = Messages directs, salons et archives
login-encrypted = Discussions chiffrées en 1:1 avec OMEMO
login-local = Un stockage local que vous contrôlez
login-welcome = Bon retour
login-signin = Se connecter
login-subtitle = Utilisez votre adresse XMPP et votre mot de passe.
common-jid = JID
common-password = Mot de passe
common-resource = Ressource
login-insecure = Autoriser les certificats TLS invalides (débogage uniquement)
login-remember = Mémoriser ce compte
login-connecting = Connexion…
login-password-note = Votre mot de passe n'est envoyé qu'à votre serveur XMPP.
login-password-placeholder = Votre mot de passe
login-error-jid = Saisissez un JID complet, par ex. utilisateur@exemple.org
login-error-password = Saisissez votre mot de passe

connection-connecting = Connexion au serveur…
connection-connecting-detail = En attente de la connexion XMPP et de l'authentification.
connection-failed = Connexion impossible
connection-disconnected = Déconnecté
connection-disconnected-detail = Vous n'êtes pas connecté au serveur XMPP.

section-home = Messages directs et salons
section-contacts = Les personnes de votre liste
section-rooms = Découvrir et gérer les salons
section-feed = Communautés et publications
section-communities = Suivre et explorer les communautés

nav-item-unread =
    { $label }, { $count ->
        [one] { $count } non lu
       *[other] { $count } non lus
    }

## Presence and account menus.
presence-title = Présence
presence-status = Statut
presence-online = En ligne
presence-away = Absent
presence-idle = Absent
presence-dnd = Ne pas déranger
presence-xa = Absence prolongée
presence-invisible = Invisible
presence-custom = Message personnalisé
presence-custom-placeholder = Que faites-vous ?
presence-priority = Priorité
presence-mark-idle = Marquer comme absent
presence-invisible-unsupported = Le mode invisible n'est pas pris en charge par ce serveur.
presence-invisible-xep = Votre serveur ne prend pas en charge le mode invisible (XEP-0186)
presence-set-custom = Définir un statut…
presence-edit-profile = Modifier le profil
presence-manage-accounts = Gérer les comptes…
presence-gateways = Passerelles
presence-sign-out = Se déconnecter

## Accounts.
accounts-title = Comptes
accounts-add = Ajouter un compte
accounts-add-connect = Ajouter et connecter
accounts-active = Actif
accounts-online = En ligne
accounts-offline = Hors ligne
accounts-notify = Notifier
accounts-switch = Changer
accounts-remove = Supprimer
accounts-password-missing = Saisissez le mot de passe du compte.
accounts-not-connected = Ce compte n'est pas connecté.\nRedémarrez Jabu pour le démarrer.
accounts-no-config = Ce compte n'a pas de configuration enregistrée.

## Settings.
settings-title = Paramètres
settings-theme = Thème
theme-light = Clair
theme-silver = Argent
theme-dark = Sombre
theme-butter = Beurre
settings-font-size = Taille de police
settings-compact = Espacement compact
settings-autostart = Lancer à l'ouverture de session
settings-notifications = Notifications de bureau
settings-microphone = Microphone
settings-camera = Caméra
settings-sfu = SFU d'appel
settings-timestamps = Afficher les horodatages
settings-link-previews = Aperçus des liens
settings-link-previews-hint = Récupère les métadonnées des pages liées lorsque vous demandez un aperçu.
settings-shortcuts = Raccourcis clavier
settings-shortcut-search = Rechercher
settings-shortcut-settings = Paramètres
settings-shortcut-sections = Changer de section
settings-shortcut-focus = Focus conversation / barre latérale
settings-shortcut-close = Fermer une boîte de dialogue
settings-shortcut-send = Envoyer un message
settings-shortcut-menu = Ouvrir un menu contextuel

## Conversations.
chat-no-conversations = Aucune conversation
chat-no-archived = Aucune conversation archivée
chat-unread-filter = Non lues
chat-search-placeholder = Rechercher des conversations
chat-no-messages = Aucun message
chat-load-older = Charger les messages plus anciens
chat-view-story = Voir la story
chat-pin = Épingler
chat-unpin = Désépingler
chat-mute-action = Mettre en sourdine
chat-unmute = Réactiver le son
chat-archive = Archiver
chat-unarchive = Désarchiver
chat-forward = Transférer
chat-react = Réagir { $emoji }
chat-saved-to = Enregistré dans { $path }
chat-unread-messages = { $count } messages non lus
chat-empty-state = Choisissez une discussion ou un salon dans la barre latérale, ou recherchez dans vos messages et contacts.
chat-pinned = Épinglé
chat-muted = En sourdine
chat-online = En ligne
chat-away = Absent
chat-dnd = Ne pas déranger
chat-xa = Absence prolongée
chat-offline = Hors ligne
chat-audio-call = Démarrer un appel audio
chat-video-call = Démarrer un appel vidéo
chat-search-messages = Rechercher des messages
chat-clear-history = Effacer l'historique de cette conversation
chat-change-subject = Modifier le sujet
chat-nudge = Envoyer une demande d'attention
chat-room-subject = Sujet du salon
chat-cancel-reply = Annuler la réponse
chat-back = Retour
chat-sending = Envoi…
chat-failed = Échec
chat-edited = modifié
chat-encrypted = Chiffré
chat-reply = Répondre
chat-reply-thread = Répondre dans le fil
chat-in-thread = Dans un fil
chat-add-reaction = Ajouter une réaction
chat-delete = Supprimer
chat-delete-everyone = Supprimer pour tout le monde
chat-download = Télécharger dans le cache des pièces jointes
chat-message = Message
chat-send-placeholder = Envoyer un message...
chat-attach = Joindre un fichier
chat-send = Envoyer le message
chat-send-sticker = Envoyer un sticker
chat-insert-emoji = Insérer un emoji
chat-stop-upload = Arrêter l'envoi des fichiers restants
chat-uploading = Envoi…
chat-attach-short = Joindre
chat-searching = Recherche…
chat-type-more = Saisissez au moins deux caractères.
chat-no-results = Aucun résultat.
chat-results-messages = Messages
chat-results-rooms = Messages de salon
chat-results-contacts = Contacts
chat-results-rooms-list = Salons
chat-results-posts = Publications
chat-results-spaces = Espaces
chat-incoming-audio = Appel audio entrant
chat-incoming-video = Appel vidéo entrant
chat-incoming-call = Appel entrant
chat-decline = Refuser
chat-accept = Accepter
chat-voice-channel = Salon vocal
chat-leave = Quitter
chat-join = Rejoindre
chat-conn-screen = Connexion à l'écran de l'autre personne…
chat-starting-screen = Démarrage du partage d'écran…
chat-starting-camera = Démarrage de la caméra…
chat-call-progress = Appel en cours
chat-camera-on = Activer la caméra
chat-camera-off = Désactiver la caméra
chat-camera-pending = Activation de la caméra…
chat-switch-camera = Changer de caméra
chat-share-screen = Partager l'écran
chat-stop-sharing = Arrêter le partage d'écran
chat-hang-up = Raccrocher
chat-voice-connected = Vocal connecté · { $count } dans le salon
chat-join-voice = Rejoignez le salon vocal. L'audio passe par le SFU du salon.
chat-join-voice-action = Rejoindre le vocal
chat-connecting = Connexion…
chat-mute-speaker = Couper le haut-parleur
chat-unmute-speaker = Réactiver le haut-parleur
chat-incoming-from =
    { $kind ->
        [audio] Appel audio entrant de { $name }
        [video] Appel vidéo entrant de { $name }
       *[other] Appel entrant de { $name }
    }
chat-incoming-kind =
    { $kind ->
        [audio] Appel audio entrant
        [video] Appel vidéo entrant
       *[other] Appel entrant
    }
notify-new-story = Nouvelle story
notify-new-post = Nouvelle publication
notify-story-title = 📝 Nouvelle story de { $author }
mute-15-minutes = 15 minutes
mute-1-hour = 1 heure
mute-8-hours = 8 heures
mute-24-hours = 24 heures

## Space, social and picker surfaces.
space-category-name = Nom de la catégorie
space-channel-name = Nom du salon
space-channel-type = Type de salon
space-category = Catégorie
space-change-avatar = Changer l'avatar
space-change-banner = Changer la bannière
space-invite = Inviter
space-configure = Configurer
space-members-todo = La gestion des membres et la configuration ne sont pas encore reliées.
space-invite-link = Lien d'invitation
space-affiliation = Affiliation
space-invite-help = Toute personne disposant de ce lien peut rejoindre l'espace. Seuls les gestionnaires peuvent accorder une affiliation.
space-send-invite = Envoyer l'invitation
space-approve = Approuver
space-deny = Refuser
space-save-config = Enregistrer la configuration
common-add = Ajouter
space-save-directories = Enregistrer les dossiers
space-link-room = Lier un salon
space-room-type = Type de salon
space-room-address = Adresse du salon
space-display-name = Nom d'affichage
space-join-auto = Rejoindre automatiquement
space-add-to = Ajouter à l'espace
space-visibility = Visibilité
space-service-invalid = L'adresse du service Spaces est invalide.
common-create = Créer
profile-full-name = Nom complet
profile-nickname = Surnom
profile-organisation = Organisation
profile-title = Titre
profile-email = E-mail
profile-phone = Téléphone
profile-website = Site web
profile-note = Note
profile-avatar = Avatar
space-confirm-destroy = Confirmer la destruction
space-destroy = Détruire
chat-clear-help = Cela supprime les messages de cet appareil et du stockage local. Les messages sur le serveur ne sont pas supprimés.
chat-clear = Effacer
commands-title = Commandes
common-next = Suivant
common-finish = Terminer
common-submit = Envoyer
contact-open-chat = Ouvrir la discussion
common-refresh = Actualiser
report-action = Signaler
contact-presence-sent = Demande de présence envoyée. En attente d'approbation.
contact-request-presence = Demander la présence
gateway-register = S'inscrire
contacts-requests = Demandes de contact
contacts-none = Aucun contact
contacts-awaiting = En attente d'approbation
contact-info = Infos
gateway-search-service = Rechercher un service
common-load-more = Charger plus
common-send-action = Envoyer
common-new = Nouveau
feed-publish = Publier
feed-new-post = Nouvelle publication
community-no-posts = Aucune publication dans cette communauté pour l'instant.
community-save = Enregistrer les paramètres
community-delete = Supprimer la communauté
feed-tags-placeholder = Étiquettes (séparées par des virgules)
feed-load-older = Charger les publications plus anciennes
story-share = Partager
story-open-media = Ouvrir le média de la story
story-encrypt-comment = Chiffrer le commentaire avec OMEMO
chat-encrypt = Chiffrer avec OMEMO
story-retake = Reprendre
story-replace = Remplacer
story-take-photo = Prendre une photo
story-cancel-camera = Annuler la caméra
story-choose = Choisissez une image, une vidéo ou prenez une photo
story-gallery = Galerie
members-kick = Exclure
members-ban = Bannir
omemo-hide = Masquer les appareils de chiffrement
omemo-devices = Appareils de chiffrement
omemo-fetch = Récupérer les clés
omemo-heartbeat = Battement de cœur
omemo-no-jids = Ce salon n'expose pas les vrais JID ou n'a aucun membre connu.
omemo-none = Aucun appareil OMEMO connu. Activez le chiffrement dans la zone de message pour les récupérer.
omemo-trust = Faire confiance
omemo-distrust = Ne plus faire confiance
sticker-load = Charger un sticker
emoji-manage = Gérer
emoji-help = Touchez un emoji pour l’ajouter ou le retirer des favoris.
emoji-remove-pack = Supprimer le pack
emoji-no-match = Aucun emoji correspondant.
emoji-no-favorites = Aucun favori. Appuyez sur Favoris pour en ajouter.
emoji-none = Aucun emoji pour le moment.

## Signalements et stickers.
report-reason = Motif
report-spam = Spam
report-abuse = Abus ou harcèlement
report-illegal = Contenu illégal
report-other = Autre
report-details = Détails (facultatif)
report-send = Envoyer le signalement
report-sending = Envoi…
report-stored = Signalement enregistré. Id { $id }, statut { $status }.
stickers-title = Stickers
stickers-import-hint = Importer un pack de stickers (.zip)
stickers-import-action = Importer
stickers-no-packs = Aucun pack de stickers.
stickers-add-hint = Ajoutez un ZIP d'images ou un pack de stickers Movim avec meta.json.
stickers-import-pack = Importer le pack de stickers

## Remaining UI labels.
space-invitee-jid = JID de la personne invitée
profile-menu-title = Profil
contact-jid = JID du contact
contact-nickname-placeholder = surnom (facultatif)
rooms-search-title = Rechercher des salons
rooms-join = Rejoindre le salon
post-like = J’aime
post-comment = Commenter
post-comment-placeholder = Écrire un commentaire…
community-pubsub = Service PubSub
community-create = Créer une communauté
community-node = Identifiant du nœud
community-title = Titre de la communauté
feed-post = Publier
feed-post-title = Titre de la publication
feed-post-body = Corps de la publication
feed-post-placeholder = Écrivez quelque chose…
feed-tags = Étiquettes
community-label = Communauté
community-description = Description
community-members = Membres de la communauté
report-this-post = Signaler cette publication
feed-edit = Modifier la publication
feed-delete = Supprimer la publication
story-title = Story
story-comment-placeholder = Envoyer un commentaire…
story-delete = Supprimer cette story
story-share-title = Partager la story
story-caption = Légende
story-caption-placeholder = Ajouter une légende…
main-no-conversation = Aucune conversation sélectionnée
main-conversation = Conversation
chat-nudge-action = Nudge
chat-preview-link = Aperçu du lien
emoji-favorite = Ajouter ou retirer des favoris
emoji-import = Importer un pack d’emojis Movim (.zip)
common-import = Importer
emoji-search = Rechercher un emoji
community-pubsub-hint = Service PubSub, par ex. pubsub.example.org
community-node-hint = Identifiant du nœud, par ex. rust
emoji-alias-hint = Alias utilisé dans les messages, par ex. :party:

feed-filter-all = Tout
feed-post-hint = Partagez quelque chose avec vos contacts...

## Dialog labels.
accounts-account-jid = JID du compte
call-connected = Connecté
space-menu = Menu de l’espace
channel-actions = Actions du salon
space-create-category = Créer une catégorie
space-create-channel = Créer un salon
space-members-tab = Membres
space-invite-title = Inviter dans l’espace
space-members-title = Membres de l’espace
space-settings-title = Paramètres de l’espace
space-rooms-title = Salons de l’espace
space-directory-placeholder = Nom du nouveau dossier
space-add-room-title = Ajouter un salon
accounts-menu = Menu du compte
space-add-title = Ajouter un espace
space-address-placeholder = Adresse ou invitation de l'espace
space-service-label = Service Spaces
space-service-placeholder = Service Spaces (automatique si vide)
space-name = Nom de l’espace
room-settings-title = Paramètres du salon
report-reason-placeholder = Motif (facultatif)
chat-clear-title = Effacer l'historique
rooms-search-public = Rechercher des salons publics
rooms-search-placeholder = Nom ou mot-clé du salon

call-authenticated = Authentifié
call-reconnecting = Reconnexion

report-title = Signaler le contenu
