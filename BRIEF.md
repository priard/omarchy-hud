# omarchy-hud: brief projektu (przekazanie z sesji badawczej, 2026-10-21)

## Cel
HUD w stylu Ubuntu Unity (2012) dla Omarchy: skrót klawiszowy na aktywnym oknie
otwiera menu Omarchy z listą wszystkich poleceń z menu TEJ aplikacji, fuzzy search,
Enter wykonuje polecenie. Docelowo: GTK, GNOME, Qt, KDE i własne aplikacje Omarchy.

## Środowisko
- Omarchy 4.5.0rc1, Hyprland 0.56.2 (konfiguracja w Lua: ~/.local/share/omarchy/default/hypr/,
  własne zmiany w ~/.config/hypr/)
- Bindy definiuje się przez `o.bind("SUPER + ALT + X", "Opis", "komenda")`
  (np. ~/.local/share/omarchy/default/hypr/bindings/utilities.lua)
- `SUPER + ALT + SPACE` jest ZAJĘTY ("Apps": `omarchy-shell omarchy.dock apps`, utilities.lua:2).
  Trzeba wybrać inny skrót (propozycja: SUPER + ALT + M) albo świadomie nadpisać ten.
- Pluginy omarchy-shell mają ograniczone API tylko do odczytu, więc logika ma działać jako
  osobny proces (CLI), wywoływany z menu lub bindu.
- Zainstalowane: gtk3, gtk4, qt6-base, kxmlgui, kconfigwidgets, libdbusmenu-glib/gtk3, python3 + gi.
  Aplikacje testowe: nautilus, xournalpp, evince, obs, libreoffice, pinta, mpv.

## Zasady pracy
- Żadnych dopisków o AI w commitach/PR (Co-Authored-By, "Generated with"); globalne hooki to wymuszają.
- NIE wpisywać tekstu/Entera przez wtype podczas testów UI (trafiło kiedyś do promptu Claude Code).
- Testy menu robić przez D-Bus (gdbus/busctl), nie przez symulację klawiatury.

## Ustalenia z badania (sprawdzone na żywo)

### Okno → menu: przez PID, bez wsparcia Hyprlanda
- Hyprland NIE implementuje protokołów appmenu (brak org_kde_kwin_appmenu i gtk_shell1 w binarce).
- Działa: `hyprctl activewindow -j` → `.pid`, potem `busctl --user list --json=short`
  → połączenia D-Bus o tym samym PID → introspekcja (`busctl --user tree <name>`).
- Plugin Hyprlanda NIE jest potrzebny.

### Wyniki dla aplikacji
| Aplikacja | Interfejs | Wynik |
|---|---|---|
| Xournal++ (GTK3, GtkApplication menubar) | org.gtk.Menus na /com/github/xournalpp/xournalpp/menus/menubar | 198 pozycji z etykietami (PL) |
| Nautilus (GTK4) | org.gtk.Actions na /org/gnome/Nautilus (app.*) i /org/gnome/Nautilus/window/N (win.*) | ok. 10 + 14 akcji, bez etykiet; Activate działa |
| Evince (GTK) | org.gtk.Actions na /org/gnome/Evince/window/1 | ok. 40 akcji, bez etykiet |
| OBS (Qt6), domyślnie (QT_QPA_PLATFORMTHEME=gtk3) | tylko /MenuBar = menu traya (SNI) | brak menu okna |
| OBS z QT_QPA_PLATFORMTHEME=generic | com.canonical.dbusmenu na /MenuBar/2 | 107 pozycji (Plik, Edycja, Widok…) |
| Zen (Firefox) | tylko MPRIS + /org/mozilla/zen/Remote | nic |
| Mattermost (Electron), Spotify | brak menu okna (Spotify: menu traya) | nic |
| foot / TUI | brak | nic |

### Przydatne komendy
```bash
# akcje GTK
gdbus call --session -d org.gnome.Nautilus -o /org/gnome/Nautilus/window/1 -m org.gtk.Actions.List
gdbus call --session -d org.gnome.Nautilus -o /org/gnome/Nautilus/window/1 \
  -m org.gtk.Actions.Activate toggle-sidebar "[]" "{}"
# menu GTK (lista grup subskrypcji 0..N)
gdbus call --session -d <unique> -o <path>/menus/menubar -m org.gtk.Menus.Start "[0,1,2,...,40]"
# Qt dbusmenu (uwaga: "--" przed -1, inaczej gdbus bierze -1 za opcję)
gdbus call --session -d <unique> -o /MenuBar/2 -m com.canonical.dbusmenu.GetLayout -- 0 -1 "['label']"
# wykonanie pozycji dbusmenu
gdbus call --session -d <unique> -o /MenuBar/2 -m com.canonical.dbusmenu.Event <id> clicked "<0>" 0
```

### Pułapki
1. Qt eksportuje menu tylko z niektórymi motywami platformy. `gtk3` (domyślny w Omarchy) NIE,
   `generic` TAK, ale zmienia wygląd aplikacji Qt. NIESPRAWDZONE: czy `kde`/plasma-integration
   eksportuje menu i zachowuje wygląd. To główna decyzja dla Qt/KDE.
2. Qt pod Waylandem NIE wywołuje com.canonical.AppMenu.Registrar.RegisterWindow (sprawdzone
   fałszywym registrarem). Menu i tak się pojawia na D-Bus, więc mapowanie zostaje przez PID.
   Do testów Qt registrar może wcale nie być potrzebny (do sprawdzenia: czy OBS z `generic`
   eksportuje menu BEZ registrara na szynie).
3. GTK4 nie eksportuje menu typu hamburger, tylko akcje (nazwy, bez etykiet).
   Etykiety tworzyć z nazw ("toggle-sidebar" → "Toggle sidebar"), a jeśli jest org.gtk.Menus, brać etykiety stamtąd.
4. Kilka okien jednej aplikacji (/window/1, /window/2) nie daje się powiązać z aktywnym oknem Hyprlanda.
   MVP: akcje aplikacji + pierwszego okna; później heurystyka.
5. Menu dynamiczne dbusmenu (ostatnie pliki itp.) wymagają AboutToShow przed GetLayout.
6. Electron, Chromium, Firefox/Zen i terminale są poza zasięgiem (brak eksportu menu pod Waylandem).
7. Ruchome PID: Electron i niektóre aplikacje mają połączenia D-Bus w procesach potomnych,
   więc trzeba sprawdzać też dzieci PID (pgrep -P). Uwaga: `setsid app & echo $!` NIE daje PID aplikacji.

## Proponowana architektura (MVP)
CLI `omarchy-hud` (Python + gi/Gio, bez nowych zależności):
1. PID aktywnego okna (hyprctl), nazwy D-Bus procesu i jego dzieci.
2. Zbiera pozycje z trzech źródeł: org.gtk.Menus (etykiety + akcja), org.gtk.Actions (app./win.),
   com.canonical.dbusmenu (Qt/KDE). Wynik to płaska lista "Plik › Eksportuj jako PDF", tylko enabled/visible.
3. Pokazuje ją w menu Omarchy / Walker w trybie dmenu (fuzzy search wbudowany).
4. Wybór → org.gtk.Actions.Activate albo dbusmenu Event "clicked".
5. Bind w ~/.config/hypr (nie w default/).

Aplikacje Omarchy (Qt Quick, bez QMenuBar): konwencja, że aplikacja wystawia org.gtk.Actions
lub dbusmenu ze swoimi poleceniami. Wtedy HUD obsłuży je bez osobnego kodu.

## Kolejność prac
1. MVP dla GTK (Xournal++ = org.gtk.Menus, Nautilus/Evince = org.gtk.Actions) + test na dbus.
2. Wybór launchera (menu omarchy-shell czy walker --dmenu) i skrótu.
3. Qt: decyzja o QT_QPA_PLATFORMTHEME (sprawdzić plasma-integration), potem dbusmenu.
4. Etykiety dla akcji GTK4, wiele okien, menu dynamiczne.
5. Konwencja dla aplikacji Omarchy.
6. Niesprawdzone: LibreOffice, natywne aplikacje KDE (żadna nie jest zainstalowana).

## Decyzje do podjęcia przez użytkownika
- Skrót (SUPER+ALT+SPACE zajęty).
- Czy zmieniamy motyw Qt dla wszystkich aplikacji.
- Docelowo osobny pakiet AUR czy PR do Omarchy.

Uwaga: tymczasowy registrar (registrar.py, ok. 20 linii) z sesji badawczej nie przetrwał; w razie potrzeby odtworzyć.
