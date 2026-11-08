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
