# Fonctionnalités — Événements programmés

Réf. : [03-modele-de-donnees](../03-modele-de-donnees.md) (ScheduledEvent) · [05-vocal-video](05-vocal-video.md#salons-stage).

## Création & gestion
- [ ] Créer un événement : **nom**, **description**, **image de couverture**, **date/heure de début** (et fin).
- [ ] **Type de lieu** : salon **vocal**, salon **stage**, ou **externe** (lieu/URL libre).
- [ ] **Récurrence** (quotidien/hebdo/mensuel, règles), fuseau horaire.
- [ ] Niveau de confidentialité (membres du serveur).
- [ ] Permissions : `CREATE_EVENTS` (créer/gérer les siens), `MANAGE_EVENTS` (gérer tous).

## Cycle de vie
- [ ] Statuts : **programmé → actif → terminé / annulé**.
- [ ] Démarrage manuel ou automatique (à l'heure / quand un intervenant rejoint le stage).
- [ ] Notification aux **intéressés** au démarrage, rappels avant le début.

## Participation
- [ ] **S'intéresser** (RSVP), compteur d'intéressés, liste des participants.
- [ ] Lien d'invitation pointant directement vers l'événement.
- [ ] Affichage des événements à venir en tête du serveur, badge « en direct ».
- [ ] Intégration **stage**/vocal : rejoindre l'événement = rejoindre le salon.

## Definition of Done
- Un organisateur planifie un événement stage hebdomadaire avec image de couverture, les membres s'y intéressent et reçoivent un rappel puis une notification au démarrage, et rejoignent le stage en un clic depuis la fiche d'événement.
