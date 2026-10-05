//! Source-keyed terminal translations. Machine-readable reports remain untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
    Fr,
    De,
    Pt,
    It,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "en" => Self::En,
            "es" => Self::Es,
            "fr" => Self::Fr,
            "de" => Self::De,
            "pt" => Self::Pt,
            "it" => Self::It,
            _ => return None,
        })
    }
    pub fn code(self) -> &'static str {
        ["en", "es", "fr", "de", "pt", "it"][self as usize]
    }
    pub fn detect() -> Self {
        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            Self::from_windows_language(unsafe { GetUserDefaultUILanguage() })
        }
        #[cfg(not(windows))]
        {
            Self::En
        }
    }
    #[cfg(any(windows, test))]
    fn from_windows_language(id: u16) -> Self {
        // PRIMARYLANGID covers regional variants, including it-IT and it-CH.
        match id & 0x3ff {
            0x0a => Self::Es,
            0x0c => Self::Fr,
            0x07 => Self::De,
            0x16 => Self::Pt,
            0x10 => Self::It,
            _ => Self::En,
        }
    }
    pub fn t(self, source: &str) -> String {
        if let Some(row) = MAINTENANCE_TEXT.iter().find(|row| row[0] == source) {
            return row[self as usize].to_owned();
        }
        for row in TEXT {
            if row[0] == source {
                return self.translation(row).to_owned();
            }
        }
        source.to_owned()
    }
    /// Translate fixed prose embedded around OS evidence, longest keys first.
    /// Single-pass replacement avoids translating text introduced by a translation.
    pub fn detail(self, source: &str) -> String {
        if MAINTENANCE_TEXT.iter().any(|row| row[0] == source) {
            return self.t(source);
        }
        if self == Self::En {
            return source.to_owned();
        }
        // Input words and action statuses are translated only as whole values,
        // never as fragments inside native evidence or longer diagnostics.
        if DETAIL_EXACT_ONLY.contains(&source) {
            return self.t(source);
        }
        let mut remaining = source;
        let mut out = String::new();
        while !remaining.is_empty() {
            let offset = source.len() - remaining.len();
            let previous = source[..offset].chars().next_back();
            if let Some(row) = TEXT
                .iter()
                .filter(|r| {
                    if DETAIL_EXACT_ONLY.contains(&r[0]) || !remaining.starts_with(r[0]) {
                        return false;
                    }
                    // Do not translate status fragments inside OS names/paths
                    // (for example "ok" inside "Notebook" or "info" in "information").
                    let word =
                        |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | '\\' | '-');
                    !(r[0].chars().next().is_some_and(word) && previous.is_some_and(word)
                        || r[0].chars().next_back().is_some_and(word)
                            && remaining[r[0].len()..].chars().next().is_some_and(word))
                })
                .max_by_key(|r| r[0].len())
            {
                out.push_str(self.translation(row));
                remaining = &remaining[row[0].len()..];
            } else {
                let ch = remaining.chars().next().unwrap();
                out.push(ch);
                remaining = &remaining[ch.len_utf8()..];
            }
        }
        out
    }
    pub fn control(self, id: &str) -> String {
        // Share the report's plain-language labels with selection and progress.
        // Unknown IDs remain verbatim rather than looking like a known control.
        let source = crate::ui::advice::control_label(id);
        if source == "Protection check" {
            return id.to_owned();
        }
        self.t(source)
    }

    fn translation(self, row: &[&'static str; 5]) -> &'static str {
        if self == Self::It {
            ITALIAN
                .iter()
                .find(|entry| entry[0] == row[0])
                .map_or(row[0], |entry| entry[1])
        } else {
            row[self as usize]
        }
    }
}

const DETAIL_EXACT_ONLY: &[&str] = &["all", "none", "complete", "opened", "returned", "running"];

// New maintenance UI copy is stored together for all six locales. Native EULAs,
// evidence, enum identifiers and versioned rule text retain their source values.
#[rustfmt::skip]
const MAINTENANCE_TEXT: &[[&str; 6]] = &[
    ["Screen changed. Review, then confirm again.", "La pantalla cambió. Revisa y confirma de nuevo.", "L’écran a changé. Vérifiez et confirmez à nouveau.", "Anzeige geändert. Prüfe und bestätige erneut.", "A tela mudou. Revise e confirme novamente.", "La schermata è cambiata. Controlla e conferma di nuovo."],
    ["Windows settings", "Configuración de Windows", "Paramètres Windows", "Windows-Einstellungen", "Configurações do Windows", "Impostazioni Windows"],
    ["You choose what changes.", "Tú eliges qué cambiar.", "Vous choisissez les changements.", "Du entscheidest, was sich ändert.", "Você escolhe o que mudar.", "Scegli tu cosa cambiare."],
    ["Overview", "Vista general", "Vue d’ensemble", "Übersicht", "Visão geral", "Panoramica"],
    ["Review", "Revisión", "Examen", "Prüfung", "Revisão", "Revisione"],
    ["Maintenance", "Mantenimiento", "Maintenance du PC", "Wartung", "Manutenção", "Manutenzione"],
    ["Diagnostics", "Diagnóstico", "Diagnostic", "Diagnose", "Diagnóstico", "Diagnostica"],
    ["Quality updates", "Actualizaciones de calidad", "Mises à jour qualité", "Qualitätsupdates", "Atualizações de qualidade", "Aggiornamenti qualitativi"],
    ["Desktop", "Escritorio", "Bureau", "Arbeitsplatz", "Área de trabalho", "Scrivania"],
    ["Your next step", "Tu próximo paso", "Votre prochaine étape", "Dein nächster Schritt", "Seu próximo passo", "Il prossimo passo"],
    ["Review the recommended set before anything changes.", "Revisa las recomendaciones antes de cambiar nada.", "Examinez les recommandations avant tout changement.", "Prüfe die Empfehlungen, bevor sich etwas ändert.", "Revise as recomendações antes de mudar algo.", "Esamina le raccomandazioni prima di cambiare qualcosa."],
    ["Choose individual fixes and review your exact selection.", "Elige correcciones y revisa tu selección exacta.", "Choisissez les corrections et examinez votre sélection exacte.", "Wähle einzelne Korrekturen und prüfe deine genaue Auswahl.", "Escolha correções e revise sua seleção exata.", "Scegli le correzioni ed esamina la selezione esatta."],
    ["Refresh the read-only protection check.", "Actualiza la comprobación de solo lectura.", "Actualisez la vérification en lecture seule.", "Aktualisiere die schreibgeschützte Schutzprüfung.", "Atualize a verificação somente leitura.", "Aggiorna il controllo in sola lettura."],
    ["Undo, maintenance, diagnostics and specialist tools.", "Deshacer, mantenimiento, diagnóstico y herramientas avanzadas.", "Annulation, maintenance, diagnostic et outils spécialisés.", "Rückgängig, Wartung, Diagnose und Spezialwerkzeuge.", "Desfazer, manutenção, diagnóstico e ferramentas avançadas.", "Annullamento, manutenzione, diagnostica e strumenti avanzati."],
    ["Return to your terminal.", "Volver a tu terminal.", "Revenir à votre terminal.", "Zurück zu deinem Terminal.", "Voltar ao seu terminal.", "Torna al terminale."],
    ["Check failed", "Comprobación fallida", "Vérification échouée", "Prüfung fehlgeschlagen", "Verificação falhou", "Controllo fallito"],
    ["Unverified", "Sin verificar", "Non vérifié", "Unbestätigt", "Não verificado", "Non verificato"],
    ["Not assessed", "Sin evaluar", "Non évalué", "Nicht bewertet", "Não avaliado", "Non valutato"],
    ["Storage blocked", "Almacenamiento bloqueado", "Stockage bloqué", "Speicher blockiert", "Armazenamento bloqueado", "Archiviazione bloccata"],
    ["{count} protected", "{count} protegidos", "{count} protégés", "{count} geschützt", "{count} protegidos", "{count} protetti"],
    ["{count} fixes", "{count} correcciones", "{count} corrections", "{count} Korrekturen", "{count} correções", "{count} correzioni"],
    ["{count} review", "{count} por revisar", "{count} à examiner", "{count} zu prüfen", "{count} para revisar", "{count} da esaminare"],
    ["{count} unknown", "{count} desconocidos", "{count} inconnus", "{count} unbekannt", "{count} desconhecidos", "{count} sconosciuti"],
    ["{count} failed", "{count} fallidos", "{count} échecs", "{count} fehlgeschlagen", "{count} falhas", "{count} falliti"],
    ["Restart: never automatic.", "Reinicio: nunca automático.", "Redémarrage : jamais automatique.", "Neustart: nie automatisch.", "Reinício: nunca automático.", "Riavvio: mai automatico."],
    ["Undo: saved settings only; later changes may block it.", "Deshacer: solo ajustes guardados; otros cambios pueden impedirlo.", "Annuler : réglages enregistrés ; des changements peuvent le bloquer.", "Rückgängig: nur gespeicherte Einstellungen; spätere Änderungen können es blockieren.", "Desfazer: só configurações salvas; outras alterações podem impedir.", "Annulla: solo impostazioni salvate; altre modifiche possono impedirlo."],
    ["Next: apply this selection, then verify the result.", "Después: aplicar esta selección y verificar el resultado.", "Ensuite : appliquer cette sélection et vérifier le résultat.", "Danach: diese Auswahl anwenden und das Ergebnis prüfen.", "Depois: aplicar esta seleção e verificar o resultado.", "Poi: applicare questa selezione e verificare il risultato."],
    ["Selected fixes: {count}", "Correcciones elegidas: {count}", "Corrections choisies : {count}", "Ausgewählte Korrekturen: {count}", "Correções escolhidas: {count}", "Correzioni scelte: {count}"],
    ["{completed} completed · {review} need review", "{completed} completados · {review} por revisar", "{completed} terminés · {review} à examiner", "{completed} abgeschlossen · {review} zu prüfen", "{completed} concluídos · {review} para revisar", "{completed} completati · {review} da esaminare"],
    ["Review and choose fixes", "Revisar y elegir correcciones", "Examiner et choisir les corrections", "Korrekturen prüfen und auswählen", "Revisar e escolher correções", "Esamina e scegli le correzioni"],
    ["Check again", "Volver a comprobar", "Vérifier à nouveau", "Erneut prüfen", "Verificar novamente", "Controlla di nuovo"],
    ["Advanced", "Avanzado", "Avancé", "Erweitert", "Avançado", "Avanzate"],
    ["Technical details and history", "Detalles técnicos e historial", "Détails techniques et historique", "Technische Details und Verlauf", "Detalhes técnicos e histórico", "Dettagli tecnici e cronologia"],
    ["Checking your PC", "Comprobando tu equipo", "Vérification de votre PC", "Dein PC wird geprüft", "Verificando seu computador", "Controllo del PC"],
    ["Checking after your changes", "Comprobando tras los cambios", "Vérification après vos modifications", "Prüfung nach deinen Änderungen", "Verificando após as alterações", "Controllo dopo le modifiche"],
    ["Applying selected fixes", "Aplicando las correcciones elegidas", "Application des corrections choisies", "Ausgewählte Korrekturen anwenden", "Aplicando as correções escolhidas", "Applicazione delle correzioni scelte"],
    ["Undoing recorded fixes", "Deshaciendo las correcciones guardadas", "Annulation des corrections enregistrées", "Gespeicherte Korrekturen rückgängig machen", "Desfazendo as correções registradas", "Annullamento delle correzioni registrate"],
    ["Keep this window open while Windows finishes.", "Mantén esta ventana abierta hasta que Windows termine.", "Gardez cette fenêtre ouverte jusqu’à la fin de Windows.", "Lass dieses Fenster offen, bis Windows fertig ist.", "Mantenha esta janela aberta até o Windows terminar.", "Tieni aperta questa finestra finché Windows non termina."],
    ["Current protection is unverified. Check again before choosing fixes.", "La protección actual no está verificada. Comprueba de nuevo antes de elegir correcciones.", "La protection actuelle n’est pas vérifiée. Vérifiez à nouveau avant de choisir des corrections.", "Der aktuelle Schutz ist unbestätigt. Prüfe erneut, bevor du Korrekturen auswählst.", "A proteção atual não está verificada. Verifique novamente antes de escolher correções.", "La protezione attuale non è verificata. Controlla di nuovo prima di scegliere correzioni."],
    ["Last protection check", "Última comprobación de protección", "Dernière vérification de protection", "Letzte Schutzprüfung", "Última verificação de proteção", "Ultimo controllo della protezione"],
    ["{fixes} fixes · {protected} protected · {review} to review", "{fixes} correcciones · {protected} protegidos · {review} por revisar", "{fixes} corrections · {protected} protégés · {review} à examiner", "{fixes} Korrekturen · {protected} geschützt · {review} zu prüfen", "{fixes} correções · {protected} protegidos · {review} para revisar", "{fixes} correzioni · {protected} protetti · {review} da esaminare"],
    ["{count} fixes selected", "{count} correcciones elegidas", "{count} corrections choisies", "{count} Korrekturen ausgewählt", "{count} correções escolhidas", "{count} correzioni scelte"],
    ["May need a restart: {count}", "Pueden necesitar reinicio: {count}", "Peuvent nécessiter un redémarrage : {count}", "Neustart möglicherweise nötig: {count}", "Podem exigir reinicialização: {count}", "Possibile riavvio necessario: {count}"],
    ["Only these named fixes will run. No automatic restart.", "Solo se ejecutarán estas correcciones. Sin reinicio automático.", "Seules ces corrections seront appliquées. Aucun redémarrage automatique.", "Nur diese genannten Korrekturen werden ausgeführt. Kein automatischer Neustart.", "Somente estas correções serão executadas. Sem reinicialização automática.", "Verranno eseguite solo queste correzioni. Nessun riavvio automatico."],
    ["Original settings are saved for Undo. Later changes can block Undo; extra tools are not included.", "Se guardan los ajustes originales para deshacer. Cambios posteriores pueden impedirlo; las herramientas adicionales no están incluidas.", "Les réglages d’origine sont conservés pour l’annulation. Des modifications ultérieures peuvent la bloquer ; les outils supplémentaires sont exclus.", "Originaleinstellungen werden zum Rückgängigmachen gespeichert. Spätere Änderungen können dies verhindern; Zusatzwerkzeuge sind ausgeschlossen.", "As configurações originais são salvas para desfazer. Alterações posteriores podem impedir isso; ferramentas extras não estão incluídas.", "Le impostazioni originali sono salvate per annullare. Modifiche successive possono impedirlo; gli strumenti aggiuntivi sono esclusi."],
    ["We will check again afterward. Completion does not guarantee every fix succeeded.", "Después volveremos a comprobar. Terminar no garantiza que todas las correcciones hayan funcionado.", "Une nouvelle vérification suivra. La fin du traitement ne garantit pas la réussite de chaque correction.", "Danach wird erneut geprüft. Der Abschluss garantiert nicht den Erfolg jeder Korrektur.", "Verificaremos novamente depois. A conclusão não garante o sucesso de todas as correções.", "Seguirà un nuovo controllo. La conclusione non garantisce il successo di ogni correzione."],
    ["Confirm selected fixes", "Confirmar las correcciones elegidas", "Confirmer les corrections choisies", "Ausgewählte Korrekturen bestätigen", "Confirmar as correções escolhidas", "Conferma le correzioni scelte"],
    ["Your selection is kept. Space toggles an item; Enter returns to the recap.", "Se conserva tu selección. Espacio cambia un elemento; Intro vuelve al resumen.", "Votre sélection est conservée. Espace coche un élément ; Entrée revient au récapitulatif.", "Deine Auswahl bleibt erhalten. Leertaste schaltet einen Eintrag um; Eingabe zeigt die Zusammenfassung.", "Sua seleção é mantida. Espaço alterna um item; Enter volta ao resumo.", "La selezione è conservata. Spazio cambia un elemento; Invio torna al riepilogo."],
    ["The post-check failed separately. Current protection is unverified; check again before more fixes.", "La comprobación posterior falló por separado. La protección actual no está verificada; comprueba antes de más correcciones.", "La vérification finale a échoué séparément. La protection actuelle reste non vérifiée ; vérifiez avant d’autres corrections.", "Die Nachprüfung ist separat fehlgeschlagen. Der aktuelle Schutz ist unbestätigt; vor weiteren Korrekturen erneut prüfen.", "A verificação posterior falhou separadamente. A proteção atual não está verificada; verifique antes de outras correções.", "Il controllo successivo è fallito separatamente. La protezione attuale non è verificata; controlla prima di altre correzioni."],
    ["Post-check complete. Review any remaining items before making more changes.", "Comprobación posterior terminada. Revisa los elementos pendientes antes de cambiar más cosas.", "Vérification finale terminée. Examinez les éléments restants avant d’autres modifications.", "Nachprüfung abgeschlossen. Prüfe verbleibende Punkte vor weiteren Änderungen.", "Verificação posterior concluída. Revise os itens restantes antes de outras alterações.", "Controllo successivo completato. Esamina gli elementi rimanenti prima di altre modifiche."],
    ["Advanced contains Undo and technical details. Only recorded hardening changes can be undone.", "Avanzado contiene Deshacer y detalles técnicos. Solo se pueden deshacer cambios de protección registrados.", "Avancé contient l’annulation et les détails techniques. Seuls les changements de protection enregistrés peuvent être annulés.", "Erweitert enthält Rückgängigmachen und technische Details. Nur protokollierte Schutzänderungen sind rückgängig machbar.", "Avançado contém Desfazer e detalhes técnicos. Somente alterações de proteção registradas podem ser desfeitas.", "Avanzate contiene Annulla e dettagli tecnici. Solo le modifiche di protezione registrate possono essere annullate."],
    ["Fix results", "Resultados de las correcciones", "Résultats des corrections", "Korrekturergebnisse", "Resultados das correções", "Risultati delle correzioni"],
    ["Check results", "Resultados de la comprobación", "Résultats de la vérification", "Prüfergebnisse", "Resultados da verificação", "Risultati del controllo"],
    ["Protection review", "Revisión de la protección", "Examen de la protection", "Schutzübersicht", "Revisão da proteção", "Riepilogo della protezione"],
    ["Only saved hardening settings are restored. Maintenance, scans and software installations are not undone.", "Solo se restauran ajustes de protección guardados. No se deshacen mantenimiento, análisis ni instalaciones.", "Seuls les réglages de protection enregistrés sont restaurés. Maintenance, analyses et installations ne sont pas annulées.", "Nur gespeicherte Schutzeinstellungen werden wiederhergestellt. Wartung, Scans und Installationen werden nicht rückgängig gemacht.", "Somente configurações de proteção salvas são restauradas. Manutenção, verificações e instalações não são desfeitas.", "Si ripristinano solo le impostazioni di protezione salvate. Manutenzione, scansioni e installazioni non vengono annullate."],
    ["Original desktop account", "Cuenta de escritorio original", "Compte de bureau d’origine", "Ursprüngliches Desktopkonto", "Conta de desktop original", "Account desktop originale"],
    ["Windows settings and next steps", "Configuración de Windows y próximos pasos", "Paramètres Windows et prochaines étapes", "Windows-Einstellungen und nächste Schritte", "Configurações do Windows e próximos passos", "Impostazioni Windows e passi successivi"],
    ["Review results", "Revisar resultados", "Examiner les résultats", "Ergebnisse prüfen", "Revisar resultados", "Esamina i risultati"],
    ["Eligible updates", "Actualizaciones aptas", "Mises à jour admissibles", "Geeignete Updates", "Atualizações elegíveis", "Aggiornamenti idonei"],
    ["Interactive console unavailable", "Consola interactiva no disponible", "Console interactive indisponible", "Interaktive Konsole nicht verfügbar", "Console interativo indisponível", "Console interattiva non disponibile"],
    ["Screen content is too large to review safely", "El contenido es demasiado grande para revisarlo de forma segura", "Le contenu est trop volumineux pour un examen sûr", "Bildschirminhalt ist für eine sichere Prüfung zu groß", "O conteúdo é grande demais para uma revisão segura", "Il contenuto è troppo grande per una revisione sicura"],
    ["Enlarge the terminal to continue. Esc goes back.", "Amplía el terminal para continuar. Esc vuelve atrás.", "Agrandissez le terminal pour continuer. Échap revient en arrière.", "Vergrößere das Terminal zum Fortfahren. Esc geht zurück.", "Amplie o terminal para continuar. Esc volta.", "Ingrandisci il terminale per continuare. Esc torna indietro."],
    ["Selected: {selected} of {total}", "Elegidos: {selected} de {total}", "Choisis : {selected} sur {total}", "Ausgewählt: {selected} von {total}", "Selecionados: {selected} de {total}", "Selezionati: {selected} di {total}"],
    ["PgUp/PgDn: read details · Esc: back", "RePág/AvPág: detalles · Esc: volver", "PgPréc/PgSuiv : détails · Échap : retour", "Bild↑/Bild↓: Details · Esc: zurück", "PgUp/PgDn: detalhes · Esc: voltar", "PagSu/PagGiù: dettagli · Esc: indietro"],
    ["PgDn: read all details before approval", "AvPág: lee todo antes de aprobar", "PgSuiv : tout lire avant accord", "Bild↓: vor Zustimmung alles lesen", "PgDn: leia tudo antes de aprovar", "PagGiù: leggi tutto prima del consenso"],
    ["Enter or Esc: back", "Intro o Esc: volver", "Entrée ou Échap : retour", "Eingabe oder Esc: zurück", "Enter ou Esc: voltar", "Invio o Esc: indietro"],
    ["↑/↓ · Space · Enter · Esc", "↑/↓ · Espacio · Intro · Esc", "↑/↓ · Espace · Entrée · Échap", "↑/↓ · Leertaste · Eingabe · Esc", "↑/↓ · Espaço · Enter · Esc", "↑/↓ · Spazio · Invio · Esc"],
    ["↑/↓ · Enter · Esc", "↑/↓ · Intro · Esc", "↑/↓ · Entrée · Échap", "↑/↓ · Eingabe · Esc", "↑/↓ · Enter: escolher · Esc", "↑/↓ · Invio · Esc"],
    ["Select between 1 and 32 distinct exact update identities.", "Selecciona entre 1 y 32 identidades exactas de actualización distintas.", "Sélectionnez entre 1 et 32 identités exactes de mise à jour distinctes.", "Wähle 1 bis 32 unterschiedliche genaue Updateidentitäten.", "Selecione entre 1 e 32 identidades exatas de atualização distintas.", "Seleziona da 1 a 32 identità esatte di aggiornamento distinte."],
    ["Managed device", "Dispositivo administrado", "Appareil géré", "Verwaltetes Gerät", "Dispositivo gerenciado", "Dispositivo gestito"],
    ["Policy indicators present", "Hay indicios de políticas", "Indices de politique présents", "Richtlinienhinweise vorhanden", "Há indicadores de políticas", "Indicatori di criteri presenti"],
    ["No management indicators observed", "No se observaron indicios de administración", "Aucun indice de gestion observé", "Keine Verwaltungshinweise beobachtet", "Nenhum indicador de gerenciamento observado", "Nessun indicatore di gestione osservato"],
    ["Management authority unknown", "Autoridad de administración desconocida", "Autorité de gestion inconnue", "Verwaltungszuständigkeit unbekannt", "Autoridade de gerenciamento desconhecida", "Autorità di gestione sconosciuta"],
    ["Prioritize supported software, antivirus, recovery access and tested backups.", "Prioriza software compatible, antivirus, acceso de recuperación y copias de seguridad probadas.", "Privilégiez les logiciels pris en charge, l’antivirus, l’accès de récupération et les sauvegardes testées.", "Priorisiere unterstützte Software, Virenschutz, Wiederherstellungszugang und getestete Sicherungen.", "Priorize software com suporte, antivírus, acesso de recuperação e backups testados.", "Dai priorità a software supportato, antivirus, accesso al recupero e backup verificati."],
    ["Keep protections enabled. Test game, anti-cheat and driver compatibility before considering narrow exceptions.", "Mantén las protecciones activas. Prueba juegos, sistemas antitrampas y controladores antes de considerar excepciones limitadas.", "Gardez les protections actives. Testez jeux, anti-triche et pilotes avant d’envisager des exceptions ciblées.", "Lass Schutzfunktionen aktiv. Teste Spiele, Anti-Cheat und Treiber vor begrenzten Ausnahmen.", "Mantenha as proteções ativas. Teste jogos, sistemas antitrapaça e drivers antes de considerar exceções limitadas.", "Mantieni attive le protezioni. Prova giochi, sistemi anti-cheat e driver prima di considerare eccezioni limitate."],
    ["Use least-privilege accounts and isolated build workspaces. Test compiler and container compatibility before changing protections.", "Usa cuentas con privilegios mínimos y entornos de compilación aislados. Prueba compiladores y contenedores antes de cambiar protecciones.", "Utilisez des comptes à privilèges minimaux et des espaces de compilation isolés. Testez compilateurs et conteneurs avant de modifier les protections.", "Nutze Konten mit minimalen Rechten und isolierte Build-Umgebungen. Teste Compiler und Container vor Schutzänderungen.", "Use contas com privilégios mínimos e ambientes de compilação isolados. Teste compiladores e contêineres antes de alterar proteções.", "Usa account con privilegi minimi e ambienti di compilazione isolati. Prova compilatori e container prima di modificare le protezioni."],
    ["Review Secure Boot, encryption recovery, VBS and remote exposure. Stage compatibility and recovery tests before changes.", "Revisa arranque seguro, recuperación del cifrado, VBS y exposición remota. Prepara pruebas de compatibilidad y recuperación antes de cambiar.", "Examinez démarrage sécurisé, récupération du chiffrement, VBS et exposition distante. Préparez des tests de compatibilité et de récupération avant les changements.", "Prüfe Secure Boot, Verschlüsselungswiederherstellung, VBS und Fernzugriff. Plane Kompatibilitäts- und Wiederherstellungstests vor Änderungen.", "Revise inicialização segura, recuperação da criptografia, VBS e exposição remota. Prepare testes de compatibilidade e recuperação antes de alterar.", "Controlla avvio protetto, recupero della crittografia, VBS ed esposizione remota. Prepara test di compatibilità e ripristino prima delle modifiche."],
    ["Journal {} requires review at byte {}: {}; original bytes retained", "El registro {} requiere revisión en el byte {}: {}; se conservan los bytes originales", "Le journal {} doit être examiné à l’octet {} : {} ; octets originaux conservés", "Journal {} muss bei Byte {} geprüft werden: {}; Originalbytes bleiben erhalten", "O registro {} requer revisão no byte {}: {}; bytes originais preservados", "Il registro {} richiede revisione al byte {}: {}; byte originali conservati"],
    ["This update is not yet offered to this device.", "Esta actualización aún no se ofrece a este equipo.", "Cette mise à jour n’est pas encore proposée à cet appareil.", "Dieses Update wird diesem Gerät noch nicht angeboten.", "Esta atualização ainda não está disponível para este dispositivo.", "Questo aggiornamento non è ancora disponibile per questo dispositivo."],
    ["JSON is available only for report commands, not interactive guides or desktop tools.", "JSON solo está disponible para informes, no para guías interactivas ni herramientas de escritorio.", "JSON est réservé aux rapports, pas aux guides interactifs ni aux outils de bureau.", "JSON ist nur für Berichte verfügbar, nicht für interaktive Anleitungen oder Desktopwerkzeuge.", "JSON está disponível apenas para relatórios, não para guias interativos ou ferramentas de desktop.", "JSON è disponibile solo per i rapporti, non per le guide interattive o gli strumenti desktop."],
    ["Open an administrator terminal in your own desktop account and run this command there. No automatic account substitution is used.", "Abre un terminal de administrador con tu propia cuenta de escritorio y ejecuta allí este comando. No se cambia de cuenta automáticamente.", "Ouvrez un terminal administrateur avec votre propre compte de bureau et exécutez cette commande. Aucun changement automatique de compte.", "Öffne ein Administratorterminal mit deinem eigenen Desktopkonto und führe den Befehl dort aus. Kein automatischer Kontowechsel.", "Abra um terminal de administrador na sua própria conta e execute este comando. Não há troca automática de conta.", "Apri un terminale amministratore con il tuo account desktop ed esegui il comando. Nessuna sostituzione automatica dell’account."],
    ["Original-user inventory requires your normal, non-administrator desktop terminal. Administrator profiles are never substituted.", "El inventario de usuario requiere tu terminal de escritorio normal, sin privilegios de administrador. Nunca se sustituye por un perfil de administrador.", "L’inventaire utilisateur exige votre terminal de bureau habituel, non administrateur. Les profils administrateur ne sont jamais substitués.", "Das Benutzerinventar erfordert dein normales Desktopterminal ohne Administratorrechte. Administratorprofile werden nie ersatzweise verwendet.", "O inventário do usuário requer seu terminal normal, sem privilégios de administrador. Perfis de administrador nunca são usados no lugar do original.", "L’inventario utente richiede il normale terminale desktop non amministratore. I profili amministratore non vengono mai usati al suo posto."],
    ["Review the exact plan and digest first. Approval does not execute it. Repairs and scans may change files or quarantine threats; there is no automatic rollback or reboot.", "Revisa primero el plan exacto y su huella. Aprobar no ejecuta el plan. Las reparaciones y análisis pueden cambiar archivos o aislar amenazas; no hay reversión ni reinicio automáticos.", "Examinez le plan exact et son empreinte. L’approbation ne l’exécute pas. Réparations et analyses peuvent modifier des fichiers ou isoler des menaces ; aucun retour arrière ni redémarrage automatique.", "Prüfe zuerst den genauen Plan und seinen Hash. Eine Genehmigung führt ihn nicht aus. Reparaturen und Scans können Dateien ändern oder Bedrohungen isolieren. Keine automatische Rücknahme oder Neustarts.", "Revise primeiro o plano exato e seu resumo criptográfico. Aprovar não executa o plano. Reparos e verificações podem alterar arquivos ou colocar ameaças em quarentena; não há reversão ou reinicialização automática.", "Controlla prima il piano esatto e la sua impronta. L’approvazione non lo esegue. Riparazioni e scansioni possono modificare file o isolare minacce; nessun ripristino o riavvio automatico."],
    ["Review every selected update, bundled update and EULA. Consent authorizes the Microsoft Windows Update source, reviewed licenses, downloads and installation. There is no automatic rollback or reboot.", "Revisa cada actualización seleccionada, sus componentes y licencias. El consentimiento autoriza la fuente Microsoft Windows Update, las licencias revisadas, la descarga e instalación. No hay reversión ni reinicio automáticos.", "Examinez chaque mise à jour, composant inclus et licence. Le consentement autorise la source Microsoft Windows Update, les licences examinées, le téléchargement et l’installation. Aucun retour arrière ni redémarrage automatique.", "Prüfe jedes ausgewählte Update, enthaltene Updates und die Lizenztexte. Die Zustimmung erlaubt die Quelle Microsoft Windows Update, geprüfte Lizenzen, Downloads und Installation. Keine automatische Rücknahme oder Neustarts.", "Revise cada atualização selecionada, os componentes incluídos e as licenças. O consentimento autoriza a origem Microsoft Windows Update, as licenças revisadas, o download e a instalação. Não há reversão ou reinicialização automática.", "Controlla ogni aggiornamento selezionato, componente incluso e licenza. Il consenso autorizza la fonte Microsoft Windows Update, le licenze esaminate, il download e l’installazione. Nessun ripristino o riavvio automatico."],
    ["Keep this window open until Windows finishes. Ctrl+C requests cancellation of subsequent work, not termination of servicing.", "Mantén esta ventana abierta hasta que Windows termine. Ctrl+C solicita cancelar el trabajo posterior, sin terminar los procesos de mantenimiento.", "Gardez cette fenêtre ouverte jusqu’à la fin. Ctrl+C demande l’annulation des étapes suivantes, sans arrêter les processus Windows.", "Lass dieses Fenster bis zum Abschluss offen. Strg+C fordert den Abbruch weiterer Schritte an und beendet keine Windows-Wartungsprozesse.", "Mantenha esta janela aberta até o Windows terminar. Ctrl+C solicita cancelar as próximas etapas, sem encerrar processos de manutenção.", "Tieni aperta questa finestra fino al termine. Ctrl+C richiede l’annullamento dei passaggi successivi, senza terminare i processi Windows."],
    ["Keep this window open until Windows finishes. This update supervisor has no cancellation API; Ctrl+C will not kill servicing.", "Mantén esta ventana abierta hasta que Windows termine. Este supervisor no permite cancelar; Ctrl+C no terminará el mantenimiento.", "Gardez cette fenêtre ouverte jusqu’à la fin. Ce superviseur ne permet pas l’annulation ; Ctrl+C ne tuera pas les processus Windows.", "Lass das Fenster bis zum Abschluss offen. Dieser Update-Supervisor bietet keine Abbruch-API; Strg+C beendet keine Wartungsprozesse.", "Mantenha esta janela aberta até o Windows terminar. Este supervisor não permite cancelamento; Ctrl+C não encerrará a manutenção.", "Tieni aperta questa finestra fino al termine. Questo supervisore non permette l’annullamento; Ctrl+C non terminerà la manutenzione."],
    ["Maintenance did not finish. Review the plan, owner policy and readiness. Use --details for technical information.", "El mantenimiento no terminó. Revisa el plan, la política del propietario y las condiciones del equipo. Usa --details para información técnica.", "La maintenance n’a pas abouti. Vérifiez le plan, la politique du propriétaire et les conditions requises. Utilisez --details pour les informations techniques.", "Die Wartung wurde nicht abgeschlossen. Prüfe Plan, Eigentümerrichtlinie und Bereitschaft. Technische Informationen mit --details.", "A manutenção não terminou. Revise o plano, a política do proprietário e as condições do dispositivo. Use --details para informações técnicas.", "La manutenzione non è terminata. Controlla piano, criteri del proprietario e condizioni richieste. Usa --details per i dati tecnici."],
    ["Explicit consent to the displayed selection", "Consentimiento explícito para la selección mostrada", "Consentement explicite à la sélection affichée", "Ausdrückliche Zustimmung zur angezeigten Auswahl", "Consentimento explícito para a seleção exibida", "Consenso esplicito alla selezione mostrata"],
    ["Consent to contacting Microsoft Windows Update", "Permitir contactar con Microsoft Windows Update", "Autoriser le contact avec Microsoft Windows Update", "Kontakt mit Microsoft Windows Update erlauben", "Permitir contato com Microsoft Windows Update", "Consenti il contatto con Microsoft Windows Update"],
    ["Accept all EULAs in this exact reviewed plan", "Aceptar todas las licencias de este plan exacto revisado", "Accepter toutes les licences de ce plan exact examiné", "Alle Lizenztexte dieses genau geprüften Plans akzeptieren", "Aceitar todas as licenças deste plano exato revisado", "Accetta tutte le licenze di questo piano esatto esaminato"],
    ["Acknowledge that automatic rollback is unavailable", "Reconocer que no hay reversión automática", "Reconnaître l’absence de retour arrière automatique", "Bestätigen, dass automatische Rücknahme nicht verfügbar ist", "Reconhecer que a reversão automática não está disponível", "Riconosci che il ripristino automatico non è disponibile"],
    ["Read-only diagnostics and compatibility advice", "Diagnóstico de solo lectura y consejos de compatibilidad", "Diagnostics en lecture seule et conseils de compatibilité", "Nur lesende Diagnose und Kompatibilitätshinweise", "Diagnóstico somente leitura e orientações de compatibilidade", "Diagnostica in sola lettura e consigli di compatibilità"],
    ["Show diagnostic profiles", "Mostrar perfiles de diagnóstico", "Afficher les profils de diagnostic", "Diagnoseprofile anzeigen", "Mostrar perfis de diagnóstico", "Mostra i profili diagnostici"],
    ["Choose diagnostics with arrow keys", "Elegir diagnósticos con las flechas", "Choisir les diagnostics avec les flèches", "Diagnosen mit Pfeiltasten wählen", "Escolher diagnósticos com as setas", "Scegli la diagnostica con le frecce"],
    ["Collect a read-only diagnostic report", "Recopilar un informe de diagnóstico de solo lectura", "Collecter un rapport de diagnostic en lecture seule", "Nur lesenden Diagnosebericht erfassen", "Coletar um relatório de diagnóstico somente leitura", "Raccogli un rapporto diagnostico in sola lettura"],
    ["Advice profile; it does not authorize changes", "Perfil de consejos; no autoriza cambios", "Profil de conseils ; n’autorise aucune modification", "Beratungsprofil; erlaubt keine Änderungen", "Perfil de orientação; não autoriza alterações", "Profilo di consigli; non autorizza modifiche"],
    ["Declared compatibility needs", "Necesidades de compatibilidad declaradas", "Besoins de compatibilité déclarés", "Angegebene Kompatibilitätsanforderungen", "Necessidades de compatibilidade declaradas", "Esigenze di compatibilità dichiarate"],
    ["Include verified original-user browser inventory", "Incluir inventario del navegador del usuario original verificado", "Inclure l’inventaire du navigateur de l’utilisateur d’origine vérifié", "Browserinventar des geprüften ursprünglichen Benutzers einschließen", "Incluir inventário do navegador do usuário original verificado", "Includi l’inventario browser dell’utente originale verificato"],
    ["Owner maintenance policy", "Política de mantenimiento del propietario", "Politique de maintenance du propriétaire", "Wartungsrichtlinie des Eigentümers", "Política de manutenção do proprietário", "Criteri di manutenzione del proprietario"],
    ["Show owner policy", "Mostrar política del propietario", "Afficher la politique du propriétaire", "Eigentümerrichtlinie anzeigen", "Mostrar política do proprietário", "Mostra i criteri del proprietario"],
    ["Reset to diagnostics-only defaults", "Restablecer los valores de solo diagnóstico", "Rétablir les valeurs de diagnostic uniquement", "Auf reine Diagnosestandards zurücksetzen", "Restaurar padrões de somente diagnóstico", "Ripristina le impostazioni di sola diagnostica"],
    ["Replace owner policy with typed settings", "Reemplazar la política con ajustes tipados", "Remplacer la politique par des paramètres typés", "Richtlinie durch typisierte Einstellungen ersetzen", "Substituir a política por configurações tipadas", "Sostituisci i criteri con impostazioni tipizzate"],
    ["Complete allowed-operation list, including dependencies", "Lista completa de operaciones permitidas, incluidas dependencias", "Liste complète des opérations autorisées, dépendances comprises", "Vollständige Liste erlaubter Vorgänge samt Abhängigkeiten", "Lista completa de operações permitidas, incluindo dependências", "Elenco completo delle operazioni consentite, incluse le dipendenze"],
    ["Repair and scan opt-in lifetime", "Duración del permiso de reparación y análisis", "Durée d’autorisation des réparations et analyses", "Gültigkeit der Reparatur- und Scanfreigabe", "Duração da autorização de reparos e verificações", "Durata del consenso a riparazioni e scansioni"],
    ["UTC window start, minutes after midnight", "Inicio UTC, minutos desde medianoche", "Début UTC, minutes après minuit", "UTC-Fensterbeginn, Minuten nach Mitternacht", "Início UTC, minutos após meia-noite", "Inizio UTC, minuti dopo mezzanotte"],
    ["UTC window end, minutes after midnight", "Fin UTC, minutos desde medianoche", "Fin UTC, minutes après minuit", "UTC-Fensterende, Minuten nach Mitternacht", "Fim UTC, minutos após meia-noite", "Fine UTC, minuti dopo mezzanotte"],
    ["Required idle time in seconds", "Inactividad necesaria en segundos", "Inactivité requise en secondes", "Erforderliche Leerlaufzeit in Sekunden", "Inatividade necessária em segundos", "Inattività richiesta in secondi"],
    ["Scoped exception: active-use, window or metered; at most 24 hours", "Excepción limitada: active-use, window o metered; máximo 24 horas", "Exception ciblée : active-use, window ou metered ; 24 heures maximum", "Begrenzte Ausnahme: active-use, window oder metered; höchstens 24 Stunden", "Exceção limitada: active-use, window ou metered; no máximo 24 horas", "Eccezione limitata: active-use, window o metered; massimo 24 ore"],
    ["Durable maintenance plans and verification", "Planes de mantenimiento persistentes y verificación", "Plans de maintenance persistants et vérification", "Dauerhafte Wartungspläne und Überprüfung", "Planos de manutenção persistentes e verificação", "Piani di manutenzione persistenti e verifica"],
    ["Choose maintenance with arrow keys", "Elegir mantenimiento con las flechas", "Choisir la maintenance avec les flèches", "Wartung mit Pfeiltasten wählen", "Escolher manutenção com as setas", "Scegli la manutenzione con le frecce"],
    ["Show supported operations and risks", "Mostrar operaciones admitidas y riesgos", "Afficher les opérations prises en charge et leurs risques", "Unterstützte Vorgänge und Risiken anzeigen", "Mostrar operações disponíveis e riscos", "Mostra operazioni supportate e rischi"],
    ["List saved plans", "Listar planes guardados", "Lister les plans enregistrés", "Gespeicherte Pläne auflisten", "Listar planos salvos", "Elenca i piani salvati"],
    ["Show an exact saved plan", "Mostrar un plan guardado exacto", "Afficher un plan enregistré exact", "Einen genauen gespeicherten Plan anzeigen", "Mostrar um plano salvo exato", "Mostra un piano salvato esatto"],
    ["Create a plan without executing it", "Crear un plan sin ejecutarlo", "Créer un plan sans l’exécuter", "Plan erstellen, ohne ihn auszuführen", "Criar um plano sem executá-lo", "Crea un piano senza eseguirlo"],
    ["Approve the exact displayed digest", "Aprobar la huella exacta mostrada", "Approuver l’empreinte exacte affichée", "Den genau angezeigten Hash genehmigen", "Aprovar o resumo exato exibido", "Approva l’impronta esatta mostrata"],
    ["Run an approved single-use plan", "Ejecutar un plan aprobado de un solo uso", "Exécuter un plan approuvé à usage unique", "Genehmigten einmaligen Plan ausführen", "Executar um plano aprovado de uso único", "Esegui un piano approvato monouso"],
    ["Verify interrupted work without replaying it", "Verificar trabajo interrumpido sin repetirlo", "Vérifier un travail interrompu sans le réexécuter", "Unterbrochene Arbeit ohne Wiederholung prüfen", "Verificar trabalho interrompido sem repeti-lo", "Verifica il lavoro interrotto senza rieseguirlo"],
    ["Exact selected Windows quality updates", "Actualizaciones de calidad de Windows seleccionadas exactamente", "Mises à jour qualité Windows sélectionnées exactement", "Genau ausgewählte Windows-Qualitätsupdates", "Atualizações de qualidade do Windows selecionadas exatamente", "Aggiornamenti qualitativi Windows selezionati esattamente"],
    ["Choose quality updates with arrow keys", "Elegir actualizaciones de calidad con las flechas", "Choisir les mises à jour qualité avec les flèches", "Qualitätsupdates mit Pfeiltasten wählen", "Escolher atualizações de qualidade com as setas", "Scegli gli aggiornamenti qualitativi con le frecce"],
    ["Show quality-update support and limits", "Mostrar disponibilidad y límites de actualizaciones", "Afficher la prise en charge et les limites des mises à jour", "Unterstützung und Grenzen der Qualitätsupdates anzeigen", "Mostrar suporte e limites das atualizações", "Mostra supporto e limiti degli aggiornamenti"],
    ["Discover eligible updates without installing", "Buscar actualizaciones aptas sin instalar", "Rechercher les mises à jour admissibles sans les installer", "Geeignete Updates ohne Installation suchen", "Buscar atualizações elegíveis sem instalar", "Cerca aggiornamenti idonei senza installarli"],
    ["Install only the exact approved selection", "Instalar solo la selección exacta aprobada", "Installer uniquement la sélection exacte approuvée", "Nur die genau genehmigte Auswahl installieren", "Instalar apenas a seleção exata aprovada", "Installa solo la selezione esatta approvata"],
    ["Verify installed identities without replaying", "Verificar identidades instaladas sin repetir", "Vérifier les identités installées sans réexécution", "Installierte Identitäten ohne Wiederholung prüfen", "Verificar identidades instaladas sem repetir", "Verifica le identità installate senza rieseguire"],
    ["Explicit --yes consent is required after reviewing the plan or policy.", "Se requiere --yes tras revisar el plan o la política.", "Le consentement --yes est requis après examen du plan ou de la politique.", "Nach Prüfung von Plan oder Richtlinie ist --yes erforderlich.", "É necessário consentir com --yes após revisar o plano ou a política.", "È richiesto --yes dopo aver esaminato piano o criteri."],
    ["Contacting Windows Update requires --accept-source.", "Contactar con Windows Update requiere --accept-source.", "Le contact avec Windows Update exige --accept-source.", "Kontakt mit Windows Update erfordert --accept-source.", "O contato com Windows Update exige --accept-source.", "Il contatto con Windows Update richiede --accept-source."],
    ["Quality-update approval and installation require --accept-eulas and --acknowledge-no-rollback.", "Aprobar e instalar actualizaciones requiere --accept-eulas y --acknowledge-no-rollback.", "L’approbation et l’installation exigent --accept-eulas et --acknowledge-no-rollback.", "Genehmigung und Installation erfordern --accept-eulas und --acknowledge-no-rollback.", "A aprovação e instalação exigem --accept-eulas e --acknowledge-no-rollback.", "Approvazione e installazione richiedono --accept-eulas e --acknowledge-no-rollback."],
    ["Choose each operation only once.", "Elige cada operación una sola vez.", "Choisissez chaque opération une seule fois.", "Wähle jeden Vorgang nur einmal.", "Escolha cada operação apenas uma vez.", "Scegli ogni operazione una sola volta."],
    ["Repairs and scans require an expiring --opt-in-for policy.", "Reparaciones y análisis requieren un permiso temporal --opt-in-for.", "Réparations et analyses exigent une autorisation temporaire --opt-in-for.", "Reparaturen und Scans erfordern eine befristete --opt-in-for-Richtlinie.", "Reparos e verificações exigem uma autorização temporária --opt-in-for.", "Riparazioni e scansioni richiedono un consenso temporaneo --opt-in-for."],
    ["The UTC maintenance window must have different start and end times.", "La ventana UTC debe tener inicio y fin distintos.", "La fenêtre UTC doit avoir un début et une fin différents.", "Beginn und Ende des UTC-Wartungsfensters müssen verschieden sein.", "A janela UTC deve ter início e fim diferentes.", "La finestra UTC deve avere inizio e fine diversi."],
    ["Exceptions must be unique and limited to allowed operations.", "Las excepciones deben ser únicas y limitarse a operaciones permitidas.", "Les exceptions doivent être uniques et limitées aux opérations autorisées.", "Ausnahmen müssen eindeutig und auf erlaubte Vorgänge begrenzt sein.", "Exceções devem ser únicas e limitadas às operações permitidas.", "Le eccezioni devono essere uniche e limitate alle operazioni consentite."],
    ["The digest must match an approved, unused plan. Use resume only for verification.", "La huella debe coincidir con un plan aprobado sin usar. Usa resume solo para verificar.", "L’empreinte doit correspondre à un plan approuvé non utilisé. Utilisez resume uniquement pour vérifier.", "Der Hash muss zu einem genehmigten, ungenutzten Plan passen. resume dient nur zur Prüfung.", "O resumo deve corresponder a um plano aprovado não utilizado. Use resume apenas para verificar.", "L’impronta deve corrispondere a un piano approvato inutilizzato. Usa resume solo per verificare."],
    ["Maintenance progress", "Progreso del mantenimiento", "Progression de la maintenance", "Wartungsfortschritt", "Progresso da manutenção", "Avanzamento della manutenzione"],
    ["Checking readiness", "Comprobando condiciones", "Vérification des conditions", "Bereitschaft prüfen", "Verificando condições", "Verifica delle condizioni"],
    ["Cancellation requested; waiting for Windows", "Cancelación solicitada; esperando a Windows", "Annulation demandée ; attente de Windows", "Abbruch angefordert; warte auf Windows", "Cancelamento solicitado; aguardando o Windows", "Annullamento richiesto; attesa di Windows"],
    ["Cancellation is unavailable for this update task; waiting for Windows", "Esta actualización no permite cancelar; esperando a Windows", "Annulation indisponible pour cette tâche ; attente de Windows", "Diese Updateaufgabe erlaubt keinen Abbruch; warte auf Windows", "Esta atualização não permite cancelamento; aguardando o Windows", "Annullamento non disponibile per questa attività; attesa di Windows"],
    ["Waiting for the Windows Update supervisor", "Esperando al supervisor de Windows Update", "Attente du superviseur Windows Update", "Warte auf den Windows-Update-Supervisor", "Aguardando o supervisor do Windows Update", "Attesa del supervisore Windows Update"],
    ["Check cached component-store health", "Comprobar estado almacenado de componentes", "Vérifier l’état en cache du magasin de composants", "Gespeicherten Komponentenspeicherzustand prüfen", "Verificar estado em cache do repositório de componentes", "Controlla lo stato memorizzato dell’archivio componenti"],
    ["Scan component-store health", "Analizar el estado de componentes", "Analyser l’état du magasin de composants", "Komponentenspeicherzustand untersuchen", "Analisar o estado do repositório de componentes", "Analizza lo stato dell’archivio componenti"],
    ["Repair component store using local content", "Reparar componentes con contenido local", "Réparer le magasin de composants avec le contenu local", "Komponentenspeicher mit lokalen Inhalten reparieren", "Reparar o repositório de componentes com conteúdo local", "Ripara l’archivio componenti con contenuti locali"],
    ["Verify protected system files", "Verificar archivos protegidos del sistema", "Vérifier les fichiers système protégés", "Geschützte Systemdateien prüfen", "Verificar arquivos protegidos do sistema", "Verifica i file di sistema protetti"],
    ["Repair protected system files", "Reparar archivos protegidos del sistema", "Réparer les fichiers système protégés", "Geschützte Systemdateien reparieren", "Reparar arquivos protegidos do sistema", "Ripara i file di sistema protetti"],
    ["Defender quick scan with existing settings", "Análisis rápido de Defender con los ajustes actuales", "Analyse rapide Defender avec les paramètres existants", "Defender-Schnellscan mit bestehenden Einstellungen", "Verificação rápida do Defender com as configurações atuais", "Scansione rapida Defender con le impostazioni esistenti"],
    ["Pending", "Pendiente", "En attente", "Ausstehend", "Pendente", "In sospeso"],
    ["Intent recorded", "Intención registrada", "Intention enregistrée", "Absicht protokolliert", "Intenção registrada", "Intento registrato"],
    ["Monitoring Windows", "Supervisando Windows", "Surveillance de Windows", "Windows wird überwacht", "Monitorando o Windows", "Monitoraggio di Windows"],
    ["Verifying", "Verificando", "Vérification", "Wird geprüft", "Verificando", "Verifica in corso"],
    ["Completed with evidence", "Finalizado con evidencia", "Terminé avec éléments de preuve", "Mit Nachweis abgeschlossen", "Concluído com evidências", "Completato con riscontri"],
    ["Owner-initiated reboot required", "Se requiere reinicio iniciado por el propietario", "Redémarrage par le propriétaire requis", "Neustart durch den Eigentümer erforderlich", "Reinicialização pelo proprietário necessária", "Riavvio da parte del proprietario richiesto"],
    ["Needs review", "Requiere revisión", "À examiner", "Prüfung erforderlich", "Requer análise", "Richiede revisione"],
    ["Failed", "Falló", "Échec", "Fehlgeschlagen", "Falhou", "Non riuscito"],
    ["Cancelled", "Cancelado", "Annulé", "Abgebrochen", "Cancelado", "Annullato"],
    ["Planned", "Planificado", "Planifié", "Geplant", "Planejado", "Pianificato"],
    ["Downloading", "Descargando", "Téléchargement", "Wird heruntergeladen", "Baixando", "Download in corso"],
    ["Downloaded", "Descargado", "Téléchargé", "Heruntergeladen", "Baixado", "Scaricato"],
    ["Installing", "Instalando", "Installation", "Wird installiert", "Instalando", "Installazione in corso"],
    ["Installed identities verified", "Identidades instaladas verificadas", "Identités installées vérifiées", "Installierte Identitäten geprüft", "Identidades instaladas verificadas", "Identità installate verificate"],
    ["Observed healthy", "Estado observado correcto", "État observé sain", "Beobachteter Zustand in Ordnung", "Estado observado saudável", "Stato osservato regolare"],
    ["Needs attention", "Requiere atención", "Nécessite une attention", "Aufmerksamkeit erforderlich", "Requer atenção", "Richiede attenzione"],
    ["Unknown", "Desconocido", "Inconnu", "Unbekannt", "Desconhecido", "Sconosciuto"],
    ["Unsupported", "No compatible", "Non pris en charge", "Nicht unterstützt", "Não suportado", "Non supportato"],
    ["Yes", "Sí", "Oui", "Ja", "Sim", "Sì"],
    ["No", "No", "Non", "Nein", "Não", "No"],
    ["Everyday use", "Uso cotidiano", "Usage quotidien", "Alltägliche Nutzung", "Uso cotidiano", "Uso quotidiano"],
    ["Gaming", "Juegos", "Jeux", "Spielen", "Jogos", "Giochi"],
    ["Software development", "Desarrollo de software", "Développement logiciel", "Softwareentwicklung", "Desenvolvimento de software", "Sviluppo software"],
    ["Higher security", "Mayor seguridad", "Sécurité renforcée", "Höhere Sicherheit", "Segurança reforçada", "Sicurezza maggiore"],
    ["Profiles change advice only. Compatibility needs never authorize disabling protection.", "Los perfiles solo cambian los consejos. Las necesidades de compatibilidad no autorizan desactivar protecciones.", "Les profils modifient uniquement les conseils. Les besoins de compatibilité n’autorisent jamais la désactivation des protections.", "Profile ändern nur Empfehlungen. Kompatibilitätsanforderungen erlauben nie das Abschalten von Schutzfunktionen.", "Os perfis alteram apenas as orientações. Necessidades de compatibilidade nunca autorizam desativar proteções.", "I profili cambiano solo i consigli. Le esigenze di compatibilità non autorizzano mai a disattivare le protezioni."],
    ["Diagnostic profile", "Perfil de diagnóstico", "Profil de diagnostic", "Diagnoseprofil", "Perfil de diagnóstico", "Profilo diagnostico"],
    ["Probes with evidence", "Comprobaciones con evidencia", "Sondes avec données", "Prüfungen mit Nachweisen", "Verificações com evidências", "Controlli con riscontri"],
    ["Unknown assessments", "Evaluaciones desconocidas", "Évaluations inconnues", "Unbekannte Bewertungen", "Avaliações desconhecidas", "Valutazioni sconosciute"],
    ["Advisory recommendations", "Recomendaciones orientativas", "Recommandations indicatives", "Unverbindliche Empfehlungen", "Recomendações informativas", "Raccomandazioni indicative"],
    ["Cached evidence is not a fresh online update scan. Unknown is not healthy. No repairs or restore tests were performed.", "Los datos almacenados no son un análisis de actualizaciones en línea reciente. Desconocido no significa correcto. No se reparó ni se probaron restauraciones.", "Les données en cache ne sont pas une recherche en ligne récente. Inconnu ne signifie pas sain. Aucune réparation ni test de restauration effectué.", "Gespeicherte Daten sind keine aktuelle Online-Updatesuche. Unbekannt bedeutet nicht gesund. Keine Reparaturen oder Wiederherstellungstests durchgeführt.", "Dados em cache não são uma busca online recente. Desconhecido não significa saudável. Nenhum reparo ou teste de restauração foi realizado.", "I dati memorizzati non sono una ricerca online recente. Sconosciuto non significa sano. Nessuna riparazione o prova di ripristino eseguita."],
    ["Use --details for rule references, compatibility advice and collection limits.", "Usa --details para referencias, consejos de compatibilidad y límites de recopilación.", "Utilisez --details pour les références, conseils de compatibilité et limites de collecte.", "Mit --details erhältst du Regelreferenzen, Kompatibilitätshinweise und Erfassungsgrenzen.", "Use --details para referências, orientações de compatibilidade e limites de coleta.", "Usa --details per riferimenti, consigli di compatibilità e limiti della raccolta."],
    ["Technical evidence and rule text (source language)", "Evidencia técnica y reglas (idioma original)", "Données techniques et règles (langue source)", "Technische Nachweise und Regeltexte (Originalsprache)", "Evidências técnicas e regras (idioma original)", "Riscontri tecnici e regole (lingua originale)"],
    ["Windows execution available", "Ejecución en Windows disponible", "Exécution Windows disponible", "Windows-Ausführung verfügbar", "Execução no Windows disponível", "Esecuzione Windows disponibile"],
    ["Application upgrades are unavailable. Use quality-updates for separately approved Windows quality updates.", "Las actualizaciones de aplicaciones no están disponibles. Usa quality-updates para actualizaciones de Windows aprobadas por separado.", "Les mises à niveau d’applications sont indisponibles. Utilisez quality-updates pour les mises à jour Windows approuvées séparément.", "Anwendungsupdates sind nicht verfügbar. Nutze quality-updates für separat genehmigte Windows-Qualitätsupdates.", "Atualizações de aplicativos não estão disponíveis. Use quality-updates para atualizações do Windows aprovadas separadamente.", "Gli aggiornamenti applicativi non sono disponibili. Usa quality-updates per aggiornamenti Windows approvati separatamente."],
    ["Approval expires (UTC Unix seconds)", "La aprobación caduca (segundos Unix UTC)", "Expiration de l’approbation (secondes Unix UTC)", "Genehmigung endet (UTC-Unix-Sekunden)", "Aprovação expira (segundos Unix UTC)", "Scadenza approvazione (secondi Unix UTC)"],
    ["Not approved", "Sin aprobar", "Non approuvé", "Nicht genehmigt", "Não aprovado", "Non approvato"],
    ["Single-use plan consumed", "Plan de un solo uso consumido", "Plan à usage unique consommé", "Einmaliger Plan verbraucht", "Plano de uso único consumido", "Piano monouso consumato"],
    ["Evidence", "Evidencia", "Éléments observés", "Nachweise", "Evidências", "Riscontri"],
    ["Native exit code", "Código de salida nativo", "Code de sortie natif", "Nativer Rückgabecode", "Código de saída nativo", "Codice di uscita nativo"],
    ["Observation", "Observación", "Observation du système", "Beobachtung", "Observação", "Osservazione"],
    ["Observation budget exceeded; no process was killed", "Tiempo de observación superado; no se terminó ningún proceso", "Délai d’observation dépassé ; aucun processus tué", "Beobachtungszeit überschritten; kein Prozess beendet", "Tempo de observação excedido; nenhum processo foi encerrado", "Tempo di osservazione superato; nessun processo terminato"],
    ["No saved plans.", "No hay planes guardados.", "Aucun plan enregistré.", "Keine gespeicherten Pläne.", "Nenhum plano salvo.", "Nessun piano salvato."],
    ["Quality-update installation available", "Instalación de actualizaciones de calidad disponible", "Installation de mises à jour qualité disponible", "Qualitätsupdate-Installation verfügbar", "Instalação de atualizações de qualidade disponível", "Installazione aggiornamenti qualitativi disponibile"],
    ["Only selected Windows security and critical quality updates are supported. Drivers, feature upgrades, previews and application upgrades are excluded.", "Solo se admiten actualizaciones de seguridad y calidad críticas seleccionadas de Windows. Se excluyen controladores, versiones de funciones, preliminares y aplicaciones.", "Seules les mises à jour qualité critiques et de sécurité Windows sélectionnées sont prises en charge. Pilotes, nouvelles versions, aperçus et applications sont exclus.", "Nur ausgewählte kritische Windows-Qualitäts- und Sicherheitsupdates werden unterstützt. Treiber, Funktionsupgrades, Vorschauen und Anwendungsupdates sind ausgeschlossen.", "Somente atualizações críticas e de segurança selecionadas do Windows são suportadas. Drivers, versões de recursos, prévias e aplicativos são excluídos.", "Sono supportati solo aggiornamenti qualitativi critici e di sicurezza Windows selezionati. Esclusi driver, nuove funzionalità, anteprime e applicazioni."],
    ["Windows Update source", "Fuente de Windows Update", "Source Windows Update", "Windows-Update-Quelle", "Origem do Windows Update", "Fonte Windows Update"],
    ["Search time (UTC Unix seconds)", "Hora de búsqueda (segundos Unix UTC)", "Heure de recherche (secondes Unix UTC)", "Suchzeit (UTC-Unix-Sekunden)", "Horário da busca (segundos Unix UTC)", "Ora ricerca (secondi Unix UTC)"],
    ["No eligible quality updates were returned. This is not proof that every update is installed.", "No se encontraron actualizaciones de calidad aptas. Esto no prueba que estén todas instaladas.", "Aucune mise à jour qualité admissible retournée. Cela ne prouve pas que tout est installé.", "Keine geeigneten Qualitätsupdates gefunden. Das beweist nicht, dass alle Updates installiert sind.", "Nenhuma atualização de qualidade elegível retornada. Isso não prova que todas estejam instaladas.", "Nessun aggiornamento qualitativo idoneo restituito. Non prova che siano tutti installati."],
    ["Earlier uncertainty retained", "Incertidumbre anterior conservada", "Incertitude antérieure conservée", "Frühere Unsicherheit beibehalten", "Incerteza anterior preservada", "Incertezza precedente conservata"],
    ["Reboot pending", "Reinicio pendiente", "Redémarrage en attente", "Neustart ausstehend", "Reinicialização pendente", "Riavvio in sospeso"],
    ["Command completed; clean integrity is not confirmed", "Comando finalizado; integridad correcta no confirmada", "Commande terminée ; intégrité saine non confirmée", "Befehl abgeschlossen; fehlerfreie Integrität nicht bestätigt", "Comando concluído; integridade saudável não confirmada", "Comando completato; integrità corretta non confermata"],
    ["Component-store health verified", "Estado de componentes verificado", "État du magasin de composants vérifié", "Komponentenspeicherzustand geprüft", "Estado do repositório de componentes verificado", "Stato dell’archivio componenti verificato"],
    ["Component-store corruption is repairable", "La corrupción de componentes se puede reparar", "La corruption du magasin de composants est réparable", "Komponentenspeicherbeschädigung ist reparierbar", "A corrupção do repositório de componentes é reparável", "Il danneggiamento dell’archivio componenti è riparabile"],
    ["Component-store corruption is not repairable", "La corrupción de componentes no se puede reparar", "La corruption du magasin de composants n’est pas réparable", "Komponentenspeicherbeschädigung ist nicht reparierbar", "A corrupção do repositório de componentes não é reparável", "Il danneggiamento dell’archivio componenti non è riparabile"],
    ["Quick-scan completion verified; threat absence is not confirmed", "Análisis rápido finalizado; ausencia de amenazas no confirmada", "Fin d’analyse rapide vérifiée ; absence de menaces non confirmée", "Abschluss des Schnellscans geprüft; Bedrohungsfreiheit nicht bestätigt", "Conclusão da verificação rápida confirmada; ausência de ameaças não confirmada", "Fine scansione rapida verificata; assenza di minacce non confermata"],
    ["Inconclusive evidence", "Evidencia no concluyente", "Données non concluantes", "Nicht eindeutige Nachweise", "Evidências inconclusivas", "Riscontri non conclusivi"],
    ["Risk", "Riesgo", "Risque", "Risiko", "Risco", "Rischio"],
    ["Diagnostic disk and CPU activity", "Actividad de diagnóstico de disco y CPU", "Activité diagnostique du disque et du processeur", "Diagnoseaktivität von Datenträger und CPU", "Atividade de diagnóstico de disco e CPU", "Attività diagnostica di disco e CPU"],
    ["System files may be replaced", "Se pueden reemplazar archivos del sistema", "Des fichiers système peuvent être remplacés", "Systemdateien können ersetzt werden", "Arquivos do sistema podem ser substituídos", "I file di sistema possono essere sostituiti"],
    ["Threats may be quarantined or remediated", "Las amenazas pueden aislarse o corregirse", "Les menaces peuvent être isolées ou corrigées", "Bedrohungen können isoliert oder behoben werden", "Ameaças podem ser isoladas ou corrigidas", "Le minacce possono essere isolate o corrette"],
    ["Rollback", "Reversión", "Retour arrière", "Rücknahme", "Reversão", "Ripristino"],
    ["No protection-setting change", "Sin cambios en ajustes de protección", "Aucun changement des paramètres de protection", "Keine Änderung von Schutzeinstellungen", "Sem alterações nas configurações de proteção", "Nessuna modifica delle impostazioni di protezione"],
    ["No automatic rollback", "Sin reversión automática", "Aucun retour arrière automatique", "Keine automatische Rücknahme", "Sem reversão automática", "Nessun ripristino automatico"],
    ["Observation budget (seconds)", "Tiempo de observación (segundos)", "Délai d’observation (secondes)", "Beobachtungszeit (Sekunden)", "Tempo de observação (segundos)", "Tempo di osservazione (secondi)"],
    ["Network access", "Acceso a la red", "Accès réseau", "Netzwerkzugriff", "Acesso à rede", "Accesso alla rete"],
    ["May require an owner-initiated reboot", "Puede requerir reinicio iniciado por el propietario", "Peut nécessiter un redémarrage par le propriétaire", "Kann einen Neustart durch den Eigentümer erfordern", "Pode exigir reinicialização pelo proprietário", "Può richiedere un riavvio da parte del proprietario"],
    ["DISM uses local repair content only and then runs an independent health scan.", "DISM usa solo contenido local y después analiza el estado de forma independiente.", "DISM utilise uniquement le contenu local, puis effectue une analyse d’état indépendante.", "DISM nutzt nur lokale Reparaturinhalte und führt danach eine unabhängige Zustandsprüfung aus.", "O DISM usa apenas conteúdo local e depois executa uma análise independente de integridade.", "DISM usa solo contenuti locali e poi esegue un’analisi indipendente dello stato."],
    ["SFC repair remains Needs review until its integrity results are reviewed; exit zero is not a clean-health claim.", "La reparación SFC requiere revisión de sus resultados; un código cero no confirma un estado correcto.", "La réparation SFC reste à examiner jusqu’à la lecture des résultats ; un code zéro ne prouve pas l’intégrité.", "SFC-Reparaturen benötigen eine Ergebnisprüfung; Rückgabecode null bestätigt keinen fehlerfreien Zustand.", "O reparo SFC requer revisão dos resultados; código zero não confirma integridade saudável.", "La riparazione SFC richiede la revisione dei risultati; il codice zero non conferma l’integrità."],
    ["Allowed operations", "Operaciones permitidas", "Opérations autorisées", "Erlaubte Vorgänge", "Operações permitidas", "Operazioni consentite"],
    ["Owner opt-in expires (UTC Unix seconds)", "El permiso caduca (segundos Unix UTC)", "Expiration du consentement (secondes Unix UTC)", "Eigentümerfreigabe endet (UTC-Unix-Sekunden)", "Permissão expira (segundos Unix UTC)", "Scadenza consenso (secondi Unix UTC)"],
    ["Diagnostics only", "Solo diagnóstico", "Diagnostic uniquement", "Nur Diagnose", "Somente diagnóstico", "Solo diagnostica"],
    ["Maintenance window (UTC)", "Ventana de mantenimiento (UTC)", "Fenêtre de maintenance (UTC)", "Wartungsfenster (UTC)", "Janela de manutenção (UTC)", "Finestra di manutenzione (UTC)"],
    ["Scoped exception", "Excepción limitada", "Exception ciblée", "Begrenzte Ausnahme", "Exceção limitada", "Eccezione limitata"],
    ["Active use", "Uso activo", "Utilisation active", "Aktive Nutzung", "Uso ativo", "Uso attivo"],
    ["Maintenance window", "Ventana de mantenimiento", "Fenêtre de maintenance", "Wartungsfenster", "Janela de manutenção", "Finestra di manutenzione"],
    ["Metered network", "Red de uso medido", "Réseau limité", "Getaktetes Netzwerk", "Rede limitada", "Rete a consumo"],
    ["Policy permission does not approve a plan. Elevation, ownership, power, storage and servicing checks cannot be bypassed.", "La política no aprueba planes. No se omiten comprobaciones de privilegios, propiedad, energía, almacenamiento ni mantenimiento.", "La politique n’approuve pas de plan. Les contrôles de privilèges, propriété, alimentation, stockage et maintenance restent obligatoires.", "Richtlinienberechtigung genehmigt keinen Plan. Prüfungen von Rechten, Zuständigkeit, Strom, Speicher und Wartung sind nicht umgehbar.", "A política não aprova planos. Verificações de privilégios, propriedade, energia, armazenamento e manutenção não podem ser ignoradas.", "I criteri non approvano piani. I controlli di privilegi, proprietà, alimentazione, spazio e manutenzione restano obbligatori."],
    ["Plan ID", "ID del plan", "ID du plan", "Plan-ID", "ID do plano", "ID del piano"],
    ["Exact plan digest", "Huella exacta del plan", "Empreinte exacte du plan", "Genauer Plan-Hash", "Resumo exato do plano", "Impronta esatta del piano"],
    ["Plan expires (UTC Unix seconds)", "El plan caduca (segundos Unix UTC)", "Expiration du plan (secondes Unix UTC)", "Plan endet (UTC-Unix-Sekunden)", "Plano expira (segundos Unix UTC)", "Scadenza piano (secondi Unix UTC)"],
    ["Dependencies", "Dependencias", "Dépendances", "Abhängigkeiten", "Dependências", "Dipendenze"],
    ["Selected update", "Actualización seleccionada", "Mise à jour sélectionnée", "Ausgewähltes Update", "Atualização selecionada", "Aggiornamento selezionato"],
    ["Bundled update", "Actualización incluida", "Mise à jour incluse", "Enthaltenes Update", "Atualização incluída", "Aggiornamento incluso"],
    ["Title", "Título", "Titre", "Titel", "Título", "Titolo"],
    ["Description", "Descripción", "Description du contenu", "Beschreibung", "Descrição", "Descrizione"],
    ["KB articles", "Artículos KB", "Articles KB", "KB-Artikel", "Artigos KB", "Articoli KB"],
    ["Maximum download (bytes)", "Descarga máxima (bytes)", "Téléchargement maximal (octets)", "Maximaler Download (Bytes)", "Download máximo (bytes)", "Download massimo (byte)"],
    ["Severity", "Gravedad", "Gravité", "Schweregrad", "Gravidade", "Gravità"],
    ["Native update metadata", "Metadatos nativos de actualización", "Métadonnées natives de mise à jour", "Native Update-Metadaten", "Metadados nativos da atualização", "Metadati nativi dell’aggiornamento"],
    ["EULA (full source text)", "Licencia EULA (texto original completo)", "Licence EULA (texte source intégral)", "EULA (vollständiger Originaltext)", "Licença EULA (texto original completo)", "Licenza EULA (testo originale completo)"],
    ["Cached Windows updates", "Actualizaciones de Windows almacenadas", "Mises à jour Windows en cache", "Gespeicherte Windows-Updates", "Atualizações do Windows em cache", "Aggiornamenti Windows memorizzati"],
    ["Update history", "Historial de actualizaciones", "Historique des mises à jour", "Updateverlauf", "Histórico de atualizações", "Cronologia aggiornamenti"],
    ["Defender health", "Estado de Defender", "État de Defender", "Defender-Zustand", "Estado do Defender", "Stato di Defender"],
    ["Defender policy", "Política de Defender", "Politique Defender", "Defender-Richtlinie", "Política do Defender", "Criteri Defender"],
    ["Device management", "Gestión del dispositivo", "Gestion de l’appareil", "Geräteverwaltung", "Gerenciamento do dispositivo", "Gestione dispositivo"],
    ["Trusted Platform Module", "Módulo de plataforma segura", "Module de plateforme sécurisée", "Vertrauenswürdiges Plattformmodul", "Módulo de plataforma confiável", "Modulo di piattaforma attendibile"],
    ["Virtualization-based security", "Seguridad basada en virtualización", "Sécurité basée sur la virtualisation", "Virtualisierungsbasierte Sicherheit", "Segurança baseada em virtualização", "Sicurezza basata sulla virtualizzazione"],
    ["Windows recovery environment", "Entorno de recuperación de Windows", "Environnement de récupération Windows", "Windows-Wiederherstellungsumgebung", "Ambiente de recuperação do Windows", "Ambiente di ripristino Windows"],
    ["Installed software", "Software instalado", "Logiciels installés", "Installierte Software", "Software instalado", "Software installato"],
    ["Browser extensions", "Extensiones del navegador", "Extensions du navigateur", "Browsererweiterungen", "Extensões do navegador", "Estensioni browser"],
    ["Physical disk health", "Estado de discos físicos", "État des disques physiques", "Zustand physischer Datenträger", "Estado dos discos físicos", "Stato dei dischi fisici"],
    ["Filesystem health", "Estado del sistema de archivos", "État du système de fichiers", "Dateisystemzustand", "Estado do sistema de arquivos", "Stato del file system"],
    ["Backup evidence", "Evidencia de copias de seguridad", "Indices de sauvegarde", "Sicherungsnachweise", "Evidências de backup", "Riscontri dei backup"],
    ["Network adapters", "Adaptadores de red", "Cartes réseau", "Netzwerkadapter", "Adaptadores de rede", "Schede di rete"],
    ["DNS configuration", "Configuración DNS", "Configuration DNS", "DNS-Konfiguration", "Configuração DNS", "Configurazione DNS"],
    ["Machine proxy", "Proxy del equipo", "Proxy de la machine", "Computerproxy", "Proxy do computador", "Proxy del computer"],
    ["Machine VPN", "VPN del equipo", "VPN de la machine", "Computer-VPN", "VPN do computador", "VPN del computer"],
    ["Service permissions", "Permisos de servicios", "Autorisations des services", "Dienstberechtigungen", "Permissões de serviços", "Autorizzazioni dei servizi"],
    ["Choose a diagnostic profile", "Elige un perfil de diagnóstico", "Choisissez un profil de diagnostic", "Diagnoseprofil wählen", "Escolha um perfil de diagnóstico", "Scegli un profilo diagnostico"],
    ["Printers", "Impresoras", "Imprimantes", "Drucker", "Impressoras", "Stampanti"],
    ["NAS and shared storage", "NAS y almacenamiento compartido", "NAS et stockage partagé", "NAS und gemeinsamer Speicher", "NAS e armazenamento compartilhado", "NAS e archiviazione condivisa"],
    ["VPN", "Red VPN", "Réseau VPN", "VPN-Verbindung", "Rede VPN", "Rete VPN"],
    ["Choose compatibility needs; this never disables protection", "Elige necesidades de compatibilidad; esto nunca desactiva la protección", "Choisissez les besoins de compatibilité ; aucune protection n’est désactivée", "Kompatibilitätsbedarf wählen; Schutzfunktionen werden nie deaktiviert", "Escolha necessidades de compatibilidade; nenhuma proteção é desativada", "Scegli le esigenze di compatibilità; nessuna protezione viene disattivata"],
    ["Browser inventory scope", "Alcance del inventario del navegador", "Portée de l’inventaire du navigateur", "Umfang des Browserinventars", "Escopo do inventário do navegador", "Ambito dell’inventario browser"],
    ["Machine diagnostics only", "Solo diagnóstico del equipo", "Diagnostics machine uniquement", "Nur Computerdiagnose", "Somente diagnóstico do computador", "Solo diagnostica del computer"],
    ["Include original-user browser inventory", "Incluir inventario del navegador del usuario original", "Inclure l’inventaire du navigateur de l’utilisateur d’origine", "Browserinventar des ursprünglichen Benutzers einschließen", "Incluir inventário do navegador do usuário original", "Includi l’inventario browser dell’utente originale"],
    ["Read-only collection; no fixes will be applied.", "Recopilación de solo lectura; no se aplican correcciones.", "Collecte en lecture seule ; aucune correction appliquée.", "Nur lesende Erfassung; keine Korrekturen werden angewendet.", "Coleta somente leitura; nenhuma correção será aplicada.", "Raccolta in sola lettura; nessuna correzione applicata."],
    ["Maintenance plans", "Planes de mantenimiento", "Plans de maintenance", "Wartungspläne", "Planos de manutenção", "Piani di manutenzione"],
    ["Supported operations and risks", "Operaciones admitidas y riesgos", "Opérations prises en charge et risques", "Unterstützte Vorgänge und Risiken", "Operações disponíveis e riscos", "Operazioni supportate e rischi"],
    ["Create a maintenance plan", "Crear un plan de mantenimiento", "Créer un plan de maintenance", "Wartungsplan erstellen", "Criar um plano de manutenção", "Crea un piano di manutenzione"],
    ["Saved maintenance plans", "Planes de mantenimiento guardados", "Plans de maintenance enregistrés", "Gespeicherte Wartungspläne", "Planos de manutenção salvos", "Piani di manutenzione salvati"],
    ["Select exact operations; dependencies will be included", "Selecciona operaciones exactas; se incluirán dependencias", "Sélectionnez les opérations exactes ; les dépendances seront incluses", "Genaue Vorgänge wählen; Abhängigkeiten werden einbezogen", "Selecione operações exatas; dependências serão incluídas", "Seleziona le operazioni esatte; le dipendenze saranno incluse"],
    ["Choose a saved plan", "Elige un plan guardado", "Choisissez un plan enregistré", "Gespeicherten Plan wählen", "Escolha um plano salvo", "Scegli un piano salvato"],
    ["Choose a plan action", "Elige una acción para el plan", "Choisissez une action pour le plan", "Planaktion wählen", "Escolha uma ação para o plano", "Scegli un’azione per il piano"],
    ["Approve displayed digest only", "Aprobar solo la huella mostrada", "Approuver uniquement l’empreinte affichée", "Nur den angezeigten Hash genehmigen", "Aprovar apenas o resumo exibido", "Approva solo l’impronta mostrata"],
    ["Run approved displayed plan", "Ejecutar el plan mostrado aprobado", "Exécuter le plan affiché approuvé", "Angezeigten genehmigten Plan ausführen", "Executar o plano exibido aprovado", "Esegui il piano mostrato approvato"],
    ["Verify attempted work only", "Solo verificar el trabajo intentado", "Vérifier uniquement le travail tenté", "Nur versuchte Arbeit prüfen", "Somente verificar o trabalho tentado", "Verifica solo il lavoro tentato"],
    ["Approve only this displayed plan and digest for 15 minutes?", "¿Aprobar solo este plan y huella durante 15 minutos?", "Approuver uniquement ce plan et son empreinte pendant 15 minutes ?", "Nur diesen Plan und Hash für 15 Minuten genehmigen?", "Aprovar apenas este plano e resumo por 15 minutos?", "Approvare solo questo piano e impronta per 15 minuti?"],
    ["Execute this exact approved plan now, with the displayed risks?", "¿Ejecutar ahora este plan exacto aprobado con los riesgos mostrados?", "Exécuter ce plan exact approuvé maintenant, avec les risques affichés ?", "Diesen genau genehmigten Plan jetzt mit den angezeigten Risiken ausführen?", "Executar agora este plano exato aprovado com os riscos exibidos?", "Eseguire ora questo piano esatto approvato con i rischi mostrati?"],
    ["Configure allowed operations", "Configurar operaciones permitidas", "Configurer les opérations autorisées", "Erlaubte Vorgänge konfigurieren", "Configurar operações permitidas", "Configura le operazioni consentite"],
    ["Add short scoped exceptions", "Añadir excepciones breves limitadas", "Ajouter de courtes exceptions ciblées", "Kurze begrenzte Ausnahmen hinzufügen", "Adicionar exceções curtas limitadas", "Aggiungi brevi eccezioni limitate"],
    ["Choose the complete allowed list, including diagnostics needed by repairs", "Elige la lista completa, incluidos diagnósticos necesarios para reparar", "Choisissez la liste complète, diagnostics nécessaires aux réparations compris", "Vollständige Liste samt für Reparaturen benötigten Diagnosen wählen", "Escolha a lista completa, incluindo diagnósticos necessários para reparos", "Scegli l’elenco completo, inclusa la diagnostica necessaria alle riparazioni"],
    ["Owner opt-in lifetime", "Duración del permiso del propietario", "Durée du consentement du propriétaire", "Gültigkeit der Eigentümerfreigabe", "Duração da permissão do proprietário", "Durata del consenso del proprietario"],
    ["One hour", "Una hora", "Une heure", "Eine Stunde", "Uma hora", "Un’ora"],
    ["One day", "Un día", "Un jour", "Ein Tag", "Um dia", "Un giorno"],
    ["Thirty days", "Treinta días", "Trente jours", "Dreißig Tage", "Trinta dias", "Trenta giorni"],
    ["01:00 to 05:00 UTC", "De 01:00 a 05:00 UTC", "De 01:00 à 05:00 UTC", "01:00 bis 05:00 UTC", "De 01:00 a 05:00 UTC", "Dalle 01:00 alle 05:00 UTC"],
    ["22:00 to 06:00 UTC", "De 22:00 a 06:00 UTC", "De 22:00 à 06:00 UTC", "22:00 bis 06:00 UTC", "De 22:00 a 06:00 UTC", "Dalle 22:00 alle 06:00 UTC"],
    ["Required idle time", "Inactividad necesaria", "Inactivité requise", "Erforderliche Leerlaufzeit", "Inatividade necessária", "Inattività richiesta"],
    ["Five minutes", "Cinco minutos", "Cinq minutes", "Fünf Minuten", "Cinco minutos", "Cinque minuti"],
    ["Fifteen minutes", "Quince minutos", "Quinze minutes", "Fünfzehn Minuten", "Quinze minutos", "Quindici minuti"],
    ["Replace owner policy with these displayed settings? Existing exceptions will be removed.", "¿Reemplazar la política con estos ajustes? Se eliminarán las excepciones existentes.", "Remplacer la politique par ces paramètres ? Les exceptions existantes seront supprimées.", "Eigentümerrichtlinie durch diese Einstellungen ersetzen? Bestehende Ausnahmen werden entfernt.", "Substituir a política por estas configurações? Exceções existentes serão removidas.", "Sostituire i criteri con queste impostazioni? Le eccezioni esistenti saranno rimosse."],
    ["Select operations for short exceptions", "Selecciona operaciones para excepciones breves", "Sélectionnez les opérations pour les courtes exceptions", "Vorgänge für kurze Ausnahmen wählen", "Selecione operações para exceções curtas", "Seleziona le operazioni per brevi eccezioni"],
    ["Select only the gates to except for 15 minutes", "Selecciona solo las condiciones a exceptuar durante 15 minutos", "Sélectionnez uniquement les conditions à exempter pendant 15 minutes", "Nur die für 15 Minuten auszunehmenden Bedingungen wählen", "Selecione apenas as condições a dispensar por 15 minutos", "Seleziona solo le condizioni da esentare per 15 minuti"],
    ["Save these exact scoped exceptions? Hard readiness checks still apply.", "¿Guardar estas excepciones exactas? Las comprobaciones obligatorias siguen activas.", "Enregistrer ces exceptions exactes ? Les contrôles obligatoires restent actifs.", "Diese genauen Ausnahmen speichern? Unverzichtbare Bereitschaftsprüfungen bleiben aktiv.", "Salvar estas exceções exatas? As verificações obrigatórias continuam ativas.", "Salvare queste eccezioni esatte? I controlli obbligatori restano attivi."],
    ["Reset owner policy to diagnostics-only defaults and remove all exceptions?", "¿Restablecer solo diagnóstico y eliminar todas las excepciones?", "Rétablir le diagnostic uniquement et supprimer toutes les exceptions ?", "Auf reine Diagnosestandards zurücksetzen und alle Ausnahmen entfernen?", "Restaurar somente diagnóstico e remover todas as exceções?", "Ripristinare la sola diagnostica e rimuovere tutte le eccezioni?"],
    ["Selected Windows quality updates", "Actualizaciones de calidad de Windows seleccionadas", "Mises à jour qualité Windows sélectionnées", "Ausgewählte Windows-Qualitätsupdates", "Atualizações de qualidade do Windows selecionadas", "Aggiornamenti qualitativi Windows selezionati"],
    ["Supported quality updates and limits", "Actualizaciones de calidad admitidas y límites", "Mises à jour qualité prises en charge et limites", "Unterstützte Qualitätsupdates und Grenzen", "Atualizações de qualidade disponíveis e limites", "Aggiornamenti qualitativi supportati e limiti"],
    ["Discover and select exact updates", "Buscar y seleccionar actualizaciones exactas", "Rechercher et sélectionner des mises à jour exactes", "Genaue Updates suchen und auswählen", "Buscar e selecionar atualizações exatas", "Cerca e seleziona aggiornamenti esatti"],
    ["Saved quality-update plans", "Planes de actualizaciones guardados", "Plans de mises à jour qualité enregistrés", "Gespeicherte Qualitätsupdatepläne", "Planos de atualizações salvos", "Piani di aggiornamento qualitativo salvati"],
    ["Contact Microsoft Windows Update for eligible update metadata? Nothing will be installed.", "¿Contactar con Microsoft Windows Update para buscar metadatos? No se instalará nada.", "Contacter Microsoft Windows Update pour les métadonnées admissibles ? Rien ne sera installé.", "Microsoft Windows Update nach geeigneten Metadaten fragen? Es wird nichts installiert.", "Contatar Microsoft Windows Update para buscar metadados elegíveis? Nada será instalado.", "Contattare Microsoft Windows Update per i metadati idonei? Non verrà installato nulla."],
    ["Select exact updates; no updates are preselected", "Selecciona actualizaciones exactas; ninguna está preseleccionada", "Sélectionnez les mises à jour exactes ; aucune présélection", "Genaue Updates wählen; keine Vorauswahl", "Selecione atualizações exatas; nenhuma está pré-selecionada", "Seleziona gli aggiornamenti esatti; nessuna preselezione"],
    ["Refresh applicability from Microsoft Windows Update and create a plan for only this selection?", "¿Actualizar la disponibilidad desde Microsoft Windows Update y crear un plan solo para esta selección?", "Actualiser l’applicabilité depuis Microsoft Windows Update et créer un plan pour cette seule sélection ?", "Anwendbarkeit bei Microsoft Windows Update aktualisieren und nur für diese Auswahl einen Plan erstellen?", "Atualizar a aplicabilidade no Microsoft Windows Update e criar um plano apenas para esta seleção?", "Aggiornare l’applicabilità da Microsoft Windows Update e creare un piano solo per questa selezione?"],
    ["Choose a quality-update plan action", "Elige una acción para el plan de actualizaciones", "Choisissez une action pour le plan de mises à jour", "Aktion für den Qualitätsupdateplan wählen", "Escolha uma ação para o plano de atualizações", "Scegli un’azione per il piano di aggiornamento"],
    ["Install the approved displayed selection", "Instalar la selección mostrada aprobada", "Installer la sélection affichée approuvée", "Angezeigte genehmigte Auswahl installieren", "Instalar a seleção exibida aprovada", "Installa la selezione mostrata approvata"],
    ["Verify installed identities only", "Solo verificar identidades instaladas", "Vérifier uniquement les identités installées", "Nur installierte Identitäten prüfen", "Somente verificar identidades instaladas", "Verifica solo le identità installate"],
    ["Accept this exact digest, Microsoft source, all displayed EULAs and no automatic rollback?", "¿Aceptar esta huella exacta, la fuente Microsoft, todas las licencias mostradas y la ausencia de reversión automática?", "Accepter cette empreinte exacte, la source Microsoft, toutes les licences affichées et l’absence de retour arrière automatique ?", "Diesen genauen Hash, die Microsoft-Quelle, alle angezeigten Lizenzen und fehlende automatische Rücknahme akzeptieren?", "Aceitar este resumo exato, a origem Microsoft, todas as licenças exibidas e a ausência de reversão automática?", "Accettare questa impronta esatta, la fonte Microsoft, tutte le licenze mostrate e l’assenza di ripristino automatico?"],
    ["Download and install this exact approved digest now, accepting its source, EULAs and rollback limits?", "¿Descargar e instalar ahora esta huella exacta aprobada, aceptando su fuente, licencias y límites de reversión?", "Télécharger et installer cette empreinte exacte approuvée maintenant, en acceptant sa source, ses licences et limites de retour arrière ?", "Diesen genau genehmigten Hash jetzt herunterladen und installieren sowie Quelle, Lizenzen und Rücknahmegrenzen akzeptieren?", "Baixar e instalar agora este resumo exato aprovado, aceitando sua origem, licenças e limites de reversão?", "Scaricare e installare ora questa impronta esatta approvata, accettando fonte, licenze e limiti di ripristino?"],
    ["Read-only diagnostics and profiles", "Diagnósticos y perfiles de solo lectura", "Diagnostics en lecture seule et profils", "Nur lesende Diagnosen und Profile", "Diagnósticos e perfis somente leitura", "Diagnostica in sola lettura e profili"],
    ["Maintenance plans and owner policy", "Planes de mantenimiento y política del propietario", "Plans de maintenance et politique du propriétaire", "Wartungspläne und Eigentümerrichtlinie", "Planos de manutenção e política do proprietário", "Piani di manutenzione e criteri del proprietario"],
];

// Each row is English, Spanish, French, German, Portuguese.
#[rustfmt::skip]
const TEXT: &[[&str; 5]] = &[
    ["{protected} of {total} checks protected", "{protected} de {total} controles protegidos", "{protected} sur {total} contrôles protégés", "{protected} von {total} Prüfungen geschützt", "{protected} de {total} verificações protegidas"],
    ["effective protection is unverified; pending transaction", "la protección efectiva no está verificada; transacción pendiente", "la protection effective n’est pas vérifiée ; transaction en attente", "wirksamer Schutz ist nicht bestätigt; ausstehende Transaktion", "a proteção efetiva não foi verificada; transação pendente"],
    ["Repair readiness blocks new changes", "El estado del equipo impide aplicar nuevas modificaciones", "L’état du PC empêche d’appliquer de nouvelles modifications", "Der Gerätezustand verhindert neue Änderungen", "O estado do dispositivo impede novas alterações"],
    ["Firewall authority is unavailable", "No se puede determinar quién gestiona el cortafuegos", "Impossible de déterminer qui gère le pare-feu", "Firewall-Zuständigkeit nicht feststellbar", "Não foi possível determinar quem gerencia o firewall"],
    ["Effective firewall evidence is unavailable", "No hay datos del estado efectivo del cortafuegos", "Les données sur l’état effectif du pare-feu sont indisponibles", "Daten zum wirksamen Firewall-Zustand nicht verfügbar", "Os dados do estado efetivo do firewall não estão disponíveis"],
    ["Firewall evidence contradicts the local preference", "Los datos del cortafuegos contradicen el ajuste local", "Les données du pare-feu contredisent le réglage local", "Firewall-Daten widersprechen der lokalen Einstellung", "Os dados do firewall contradizem a configuração local"],
    ["Firewall evidence is invalid for this control", "Los datos del cortafuegos no son válidos para este control", "Les données du pare-feu ne sont pas valides pour ce contrôle", "Firewall-Daten sind für diese Kontrolle ungültig", "Os dados do firewall não são válidos para este controle"],
    ["Firewall evidence does not match the control", "Los datos del cortafuegos no corresponden al control", "Les données du pare-feu ne correspondent pas au contrôle", "Firewall-Daten passen nicht zur Kontrolle", "Os dados do firewall não correspondem ao controle"],
    ["Nonlocal firewall authority cannot be eligible", "Una autoridad de cortafuegos no local no permite cambios", "Une autorité de pare-feu non locale ne permet pas les modifications", "Nichtlokale Firewall-Zuständigkeit erlaubt keine Änderungen", "Uma autoridade de firewall não local não permite alterações"],
    ["Relevant policy is configured: assessment only", "Hay una directiva pertinente configurada: solo evaluación", "Une stratégie pertinente est configurée : évaluation uniquement", "Relevante Richtlinie konfiguriert: nur Bewertung", "Há uma política pertinente configurada: apenas avaliação"],
    ["EffectiveFirewallMismatch", "El estado efectivo del cortafuegos no coincide con el ajuste guardado", "L’état effectif du pare-feu ne correspond pas au réglage enregistré", "Wirksamer Firewall-Zustand stimmt nicht mit gespeicherter Einstellung überein", "O estado efetivo do firewall não corresponde à configuração salva"],
    ["EffectiveFirewallUnavailable", "No se pudo verificar el estado efectivo del cortafuegos", "L’état effectif du pare-feu n’a pas pu être vérifié", "Wirksamer Firewall-Zustand konnte nicht geprüft werden", "Não foi possível verificar o estado efetivo do firewall"],
    ["Firewall stored profile cannot be established", "No se puede determinar el perfil guardado del cortafuegos", "Impossible de déterminer le profil enregistré du pare-feu", "Gespeichertes Firewallprofil nicht feststellbar", "Não foi possível determinar o perfil salvo do firewall"],
    ["Firewall inbound preference is not readable", "No se puede leer el ajuste de entrada del cortafuegos", "Le réglage entrant du pare-feu est illisible", "Firewall-Einstellung für eingehenden Verkehr nicht lesbar", "Não foi possível ler a configuração de entrada do firewall"],
    ["Unknown firewall control", "Control de cortafuegos desconocido", "Contrôle du pare-feu inconnu", "Unbekannte Firewallkontrolle", "Controle de firewall desconhecido"],
    ["Firewall effective profile cannot be established", "No se puede determinar el perfil efectivo del cortafuegos", "Impossible de déterminer le profil effectif du pare-feu", "Wirksames Firewallprofil nicht feststellbar", "Não foi possível determinar o perfil efetivo do firewall"],
    ["Firewall enabled value must be a boolean", "El valor de activación del cortafuegos debe ser booleano", "La valeur d’activation du pare-feu doit être un booléen", "Firewall-Aktivierungswert muss ein Wahrheitswert sein", "O valor de ativação do firewall deve ser booleano"],
    ["Fix recommended", "Aplicar las correcciones recomendadas", "Appliquer les corrections recommandées", "Empfohlene Korrekturen anwenden", "Aplicar as correções recomendadas"],
    ["The new check could not finish. Check again before choosing more fixes. Undo is still available.", "La nueva revisión no pudo terminar. Vuelve a comprobar antes de elegir más correcciones. Puedes seguir deshaciendo cambios.", "La nouvelle vérification n’a pas abouti. Vérifiez à nouveau avant de choisir d’autres corrections. L’annulation reste disponible.", "Die neue Prüfung konnte nicht abgeschlossen werden. Prüfe erneut, bevor du weitere Korrekturen auswählst. Rückgängigmachen bleibt verfügbar.", "A nova verificação não terminou. Verifique novamente antes de escolher mais correções. Você ainda pode desfazer alterações."],
    ["Only these fixes will be applied. Some changes may need a restart. Extra tools and software installs are not included.", "Solo se aplicarán estas correcciones. Algunos cambios pueden necesitar un reinicio. No se incluyen herramientas adicionales ni instalaciones de software.", "Seules ces corrections seront appliquées. Certaines modifications peuvent nécessiter un redémarrage. Les outils supplémentaires et installations de logiciels sont exclus.", "Nur diese Korrekturen werden angewendet. Manche Änderungen brauchen einen Neustart. Zusatzwerkzeuge und Softwareinstallationen sind nicht enthalten.", "Somente estas correções serão aplicadas. Algumas alterações podem exigir reinicialização. Ferramentas extras e instalações de software não estão incluídas."],
    ["Apply these fixes?", "¿Aplicar estas correcciones?", "Appliquer ces corrections ?", "Diese Korrekturen anwenden?", "Aplicar estas correções?"],
    ["Apply these fixes", "Aplicar estas correcciones", "Appliquer ces corrections", "Diese Korrekturen anwenden", "Aplicar estas correções"],
    ["Change selection", "Cambiar la selección", "Modifier la sélection", "Auswahl ändern", "Alterar a seleção"],
    ["Choose the fixes to keep. The boxes start unchecked.", "Elige las correcciones que quieras conservar. Las casillas empiezan sin marcar.", "Choisissez les corrections à conserver. Aucune case n’est cochée au départ.", "Wähle die Korrekturen, die du behalten möchtest. Zu Beginn ist kein Kästchen markiert.", "Escolha as correções que deseja manter. As caixas começam desmarcadas."],
    ["Rechecking your protection", "Volviendo a comprobar tu protección", "Nouvelle vérification de votre protection", "Dein Schutz wird erneut geprüft", "Verificando sua proteção novamente"],
    ["Technical details of the latest check failure:", "Detalles técnicos del último fallo de comprobación:", "Détails techniques du dernier échec de vérification :", "Technische Details der zuletzt fehlgeschlagenen Prüfung:", "Detalhes técnicos da última falha de verificação:"],
    ["Protected by Windows", "Protegido por Windows", "Protégé par Windows", "Durch Windows geschützt", "Protegido pelo Windows"],
    ["Windows is already providing this firewall protection. No change is needed.", "Windows ya ofrece esta protección del cortafuegos. No hace falta cambiar nada.", "Windows assure déjà cette protection du pare-feu. Aucune modification n’est nécessaire.", "Windows bietet diesen Firewall-Schutz bereits. Keine Änderung nötig.", "O Windows já oferece esta proteção do firewall. Nenhuma alteração é necessária."],
    ["The active firewall setting could not be verified. Check again before making changes.", "No se pudo verificar el ajuste activo del cortafuegos. Vuelve a comprobar antes de hacer cambios.", "Le réglage actif du pare-feu n’a pas pu être vérifié. Vérifiez à nouveau avant toute modification.", "Die aktive Firewall-Einstellung konnte nicht geprüft werden. Prüfe erneut, bevor du Änderungen vornimmst.", "Não foi possível verificar a configuração ativa do firewall. Verifique novamente antes de fazer alterações."],
    ["For your information", "Para tu información", "À titre d’information", "Zu deiner Information", "Para sua informação"],
    ["More information", "Más información", "Plus d’informations", "Weitere Informationen", "Mais informações"],
    ["Device check", "Comprobación del equipo", "Vérification du PC", "Geräteprüfung", "Verificação do dispositivo"],
    ["Before making changes", "Antes de hacer cambios", "Avant de modifier", "Vor Änderungen", "Antes de fazer alterações"],
    ["Windows drive", "Unidad de Windows", "Disque Windows", "Windows-Laufwerk", "Unidade do Windows"],
    ["Saved changes drive", "Unidad de cambios guardados", "Disque des modifications enregistrées", "Laufwerk für gespeicherte Änderungen", "Unidade das alterações salvas"],
    ["Free space unknown", "Espacio libre desconocido", "Espace libre inconnu", "Freier Speicher unbekannt", "Espaço livre desconhecido"],
    ["{gb} GB free", "{gb} GB libres", "{gb} Go libres", "{gb} GB frei", "{gb} GB livres"],
    ["Disk is read-only. Fixes will wait.", "El disco es de solo lectura. Las correcciones tendrán que esperar.", "Le disque est en lecture seule. Les corrections devront attendre.", "Das Laufwerk ist schreibgeschützt. Korrekturen müssen warten.", "O disco é somente leitura. As correções terão que esperar."],
    ["No space for saved changes. Fixes will wait.", "No hay espacio para guardar los cambios. Las correcciones tendrán que esperar.", "Aucun espace pour enregistrer les modifications. Les corrections devront attendre.", "Kein Speicherplatz für gespeicherte Änderungen. Korrekturen müssen warten.", "Não há espaço para salvar as alterações. As correções terão que esperar."],
    ["Power information unknown", "Información de alimentación desconocida", "Informations d’alimentation inconnues", "Stromversorgungsdaten unbekannt", "Informações de energia desconhecidas"],
    ["Plugged in", "Conectado a la corriente", "Branché sur secteur", "Am Stromnetz", "Conectado à tomada"],
    ["Not plugged in", "Sin conexión a la corriente", "Non branché sur secteur", "Nicht am Stromnetz", "Fora da tomada"],
    ["Power source unknown", "Fuente de alimentación desconocida", "Source d’alimentation inconnue", "Stromquelle unbekannt", "Fonte de energia desconhecida"],
    ["Battery: not applicable", "Batería: no aplicable", "Batterie : sans objet", "Akku: nicht zutreffend", "Bateria: não se aplica"],
    ["Battery: {percent}%", "Batería: {percent}%", "Batterie : {percent}%", "Akku: {percent}%", "Bateria: {percent}%"],
    ["Low battery. Connect power before making changes.", "Batería baja. Conecta el equipo a la corriente antes de hacer cambios.", "Batterie faible. Branchez le PC avant de faire des modifications.", "Akku schwach. Schließe das Gerät vor Änderungen ans Stromnetz an.", "Bateria fraca. Conecte à tomada antes de fazer alterações."],
    ["Battery level unknown", "Nivel de batería desconocido", "Niveau de batterie inconnu", "Akkustand unbekannt", "Nível da bateria desconhecido"],
    ["Battery information unknown", "Información de batería desconocida", "Informations de batterie inconnues", "Akkudaten unbekannt", "Informações da bateria desconhecidas"],
    ["Windows Update needs a restart. Save your work and restart when ready.", "Windows Update necesita un reinicio. Guarda tu trabajo y reinicia cuando puedas.", "Windows Update nécessite un redémarrage. Enregistrez votre travail et redémarrez quand vous êtes prêt.", "Windows Update benötigt einen Neustart. Speichere deine Arbeit und starte neu, wenn du bereit bist.", "O Windows Update precisa de uma reinicialização. Salve seu trabalho e reinicie quando puder."],
    ["No update restart pending", "No hay reinicio pendiente por actualizaciones", "Aucun redémarrage de mise à jour en attente", "Kein Update-Neustart ausstehend", "Nenhuma reinicialização de atualização pendente"],
    ["Update restart status unknown", "Estado del reinicio por actualizaciones desconocido", "État du redémarrage de mise à jour inconnu", "Status des Update-Neustarts unbekannt", "Estado da reinicialização de atualização desconhecido"],
    ["Readiness evidence", "Datos de preparación del equipo", "Données de préparation du PC", "Daten zur Gerätebereitschaft", "Dados de preparação do dispositivo"],
    ["Firewall evidence", "Datos del cortafuegos", "Données du pare-feu", "Firewall-Daten", "Dados do firewall"],
    ["Keep Secblitz up to date", "Mantén Secblitz al día", "Gardez Secblitz à jour", "Secblitz aktuell halten", "Mantenha o Secblitz atualizado"],
    ["Check for Secblitz updates", "Buscar actualizaciones de Secblitz", "Rechercher des mises à jour de Secblitz", "Nach Secblitz-Updates suchen", "Buscar atualizações do Secblitz"],
    ["Show the latest update status", "Mostrar el último estado de actualización", "Afficher le dernier état de mise à jour", "Letzten Updatestatus anzeigen", "Mostrar o último estado da atualização"],
    ["Output raw JSON reports (including update check/status)", "Mostrar informes JSON sin traducir (incluye update check/status)", "Afficher les rapports JSON bruts (y compris update check/status)", "Unübersetzte JSON-Berichte ausgeben (einschließlich update check/status)", "Mostrar relatórios JSON sem tradução (incluindo update check/status)"],
    ["JSON is available only for audit, apply, revert, history, update check and update status.", "JSON solo está disponible para audit, apply, revert, history, update check y update status.", "JSON est disponible uniquement pour audit, apply, revert, history, update check et update status.", "JSON ist nur für audit, apply, revert, history, update check und update status verfügbar.", "JSON está disponível apenas para audit, apply, revert, history, update check e update status."],
    ["No update information yet.", "Todavía no hay información sobre actualizaciones.", "Aucune information de mise à jour pour le moment.", "Noch keine Updateinformationen vorhanden.", "Ainda não há informações sobre atualizações."],
    ["Updates aren't available for this installation.", "Las actualizaciones no están disponibles para esta instalación.", "Les mises à jour ne sont pas disponibles pour cette installation.", "Für diese Installation sind keine Updates verfügbar.", "As atualizações não estão disponíveis para esta instalação."],
    ["Secblitz is up to date.", "Secblitz está al día.", "Secblitz est à jour.", "Secblitz ist aktuell.", "O Secblitz está atualizado."],
    ["We'll try updating when Secblitz is closed.", "Intentaremos actualizar cuando Secblitz esté cerrado.", "Nous réessaierons la mise à jour lorsque Secblitz sera fermé.", "Wir versuchen das Update, wenn Secblitz geschlossen ist.", "Tentaremos atualizar quando o Secblitz estiver fechado."],
    ["Your update is ready. Close Secblitz so the installer can continue.", "Tu actualización está lista. Cierra Secblitz para que el instalador pueda continuar.", "Votre mise à jour est prête. Fermez Secblitz pour que l’installation puisse continuer.", "Dein Update ist bereit. Schließe Secblitz, damit die Installation fortfahren kann.", "Sua atualização está pronta. Feche o Secblitz para que o instalador possa continuar."],
    ["Secblitz was updated.", "Secblitz se ha actualizado.", "Secblitz a été mis à jour.", "Secblitz wurde aktualisiert.", "O Secblitz foi atualizado."],
    ["The update could not be completed.", "No se pudo completar la actualización.", "La mise à jour n’a pas pu être terminée.", "Das Update konnte nicht abgeschlossen werden.", "Não foi possível concluir a atualização."],
    ["Run update commands from an administrator terminal.", "Ejecuta los comandos de actualización desde una terminal de administrador.", "Exécutez les commandes de mise à jour depuis un terminal administrateur.", "Führe Updatebefehle in einem Administratorterminal aus.", "Execute os comandos de atualização em um terminal de administrador."],
    ["Run update status --details from an administrator terminal for more information.", "Ejecuta update status --details desde una terminal de administrador para obtener más información.", "Exécutez update status --details depuis un terminal administrateur pour en savoir plus.", "Führe update status --details in einem Administratorterminal aus, um mehr zu erfahren.", "Execute update status --details em um terminal de administrador para obter mais informações."],
    ["Terminal is too short to display a menu", "La ventana del terminal es demasiado baja para mostrar el menú", "La fenêtre du terminal n’est pas assez haute pour afficher le menu", "Das Terminalfenster ist zu niedrig, um das Menü anzuzeigen", "A janela do terminal é baixa demais para exibir o menu"],
    ["Press Enter to confirm, or Esc to cancel.", "Pulsa Intro para confirmar o Esc para cancelar.", "Appuyez sur Entrée pour confirmer ou sur Échap pour annuler.", "Drücke Eingabe zum Bestätigen oder Esc zum Abbrechen.", "Pressione Enter para confirmar ou Esc para cancelar."],
    ["Invalid menu default", "Opción predeterminada del menú no válida", "Choix par défaut du menu non valide", "Ungültige Standardauswahl im Menü", "Opção padrão do menu inválida"],
    ["Invalid menu selection", "Selección del menú no válida", "Sélection du menu non valide", "Ungültige Menüauswahl", "Seleção do menu inválida"],
    ["validated menu index", "índice del menú validado", "indice de menu validé", "geprüfter Menüindex", "índice do menu validado"],
    ["Review protection and next steps", "Revisar mi protección y próximos pasos", "Voir ma protection et les prochaines étapes", "Schutz und nächste Schritte ansehen", "Ver minha proteção e os próximos passos"],
    ["Check my PC again", "Revisar mi PC de nuevo", "Vérifier à nouveau mon PC", "Meinen PC erneut prüfen", "Verificar meu PC novamente"],
    ["Undo my last fixes", "Deshacer mis últimas correcciones", "Annuler mes dernières corrections", "Meine letzten Korrekturen rückgängig machen", "Desfazer minhas últimas correções"],
    ["Extra tools", "Más herramientas", "Outils supplémentaires", "Weitere Werkzeuge", "Mais ferramentas"],
    ["Technical details (optional)", "Detalles técnicos (opcional)", "Détails techniques (facultatif)", "Technische Details (optional)", "Detalhes técnicos (opcional)"],
    ["Exit", "Salir", "Quitter", "Beenden", "Sair"],
    ["Back", "Volver", "Retour", "Zurück", "Voltar"],
    ["Generate a password", "Crear una contraseña", "Créer un mot de passe", "Ein Passwort erzeugen", "Criar uma senha"],
    ["Install Bitwarden (optional password manager)", "Instalar Bitwarden (gestor de contraseñas opcional)", "Installer Bitwarden (gestionnaire de mots de passe facultatif)", "Bitwarden installieren (optionaler Passwortmanager)", "Instalar Bitwarden (gerenciador de senhas opcional)"],
    ["Install and start optional monitoring", "Instalar e iniciar la supervisión opcional", "Installer et démarrer la surveillance facultative", "Optionale Überwachung installieren und starten", "Instalar e iniciar o monitoramento opcional"],
    ["Update Microsoft Defender protection", "Actualizar la protección de Microsoft Defender", "Mettre à jour la protection Microsoft Defender", "Microsoft-Defender-Schutz aktualisieren", "Atualizar a proteção do Microsoft Defender"],
    ["Run a Microsoft Defender quick scan", "Ejecutar un análisis rápido de Microsoft Defender", "Lancer une analyse rapide Microsoft Defender", "Microsoft-Defender-Schnellüberprüfung starten", "Executar uma verificação rápida do Microsoft Defender"],
    ["Open Windows Update settings", "Abrir la configuración de Windows Update", "Ouvrir les paramètres de Windows Update", "Windows-Update-Einstellungen öffnen", "Abrir as configurações do Windows Update"],
    ["Open Windows Security settings", "Abrir Seguridad de Windows", "Ouvrir Sécurité Windows", "Windows-Sicherheit öffnen", "Abrir Segurança do Windows"],
    ["Open device encryption / BitLocker settings", "Abrir el cifrado del dispositivo / BitLocker", "Ouvrir le chiffrement de l’appareil / BitLocker", "Geräteverschlüsselung / BitLocker öffnen", "Abrir a criptografia do dispositivo / BitLocker"],
    ["Open sign-in settings", "Abrir las opciones de inicio de sesión", "Ouvrir les options de connexion", "Anmeldeoptionen öffnen", "Abrir as opções de entrada"],
    ["Use ↑/↓ to move, Enter to choose, Esc to go back.", "Usa ↑/↓ para moverte, Intro para elegir y Esc para volver.", "Utilisez ↑/↓ pour vous déplacer, Entrée pour choisir et Échap pour revenir.", "Mit ↑/↓ navigieren, mit Eingabe auswählen, mit Esc zurückgehen.", "Use ↑/↓ para navegar, Enter para escolher e Esc para voltar."],
    ["Use ↑/↓ to move, Space to select, Enter to continue, Esc to cancel.", "Usa ↑/↓ para moverte, Espacio para marcar, Intro para continuar y Esc para cancelar.", "Utilisez ↑/↓ pour vous déplacer, Espace pour sélectionner, Entrée pour continuer et Échap pour annuler.", "Mit ↑/↓ navigieren, mit der Leertaste auswählen, mit Eingabe fortfahren, mit Esc abbrechen.", "Use ↑/↓ para navegar, Espaço para selecionar, Enter para continuar e Esc para cancelar."],
    ["Yes, continue", "Sí, continuar", "Oui, continuer", "Ja, fortfahren", "Sim, continuar"],
    ["No, go back", "No, volver", "Non, revenir", "Nein, zurück", "Não, voltar"],
    ["Choose an action", "Elige una acción", "Choisissez une action", "Wähle eine Aktion", "Escolha uma ação"],
    ["Choose what to fix", "Elige qué corregir", "Choisissez quoi corriger", "Wähle aus, was korrigiert wird", "Escolha o que corrigir"],
    ["Select the fixes you want.", "Marca las correcciones que quieras.", "Sélectionnez les corrections souhaitées.", "Wähle die gewünschten Korrekturen aus.", "Selecione as correções que deseja."],
    ["Open Windows Update settings now?", "¿Abrir ahora la configuración de Windows Update?", "Ouvrir les paramètres de Windows Update maintenant ?", "Windows-Update-Einstellungen jetzt öffnen?", "Abrir as configurações do Windows Update agora?"],
    ["Open Windows Security settings now?", "¿Abrir ahora Seguridad de Windows?", "Ouvrir Sécurité Windows maintenant ?", "Windows-Sicherheit jetzt öffnen?", "Abrir Segurança do Windows agora?"],
    ["Open device encryption settings now?", "¿Abrir ahora el cifrado del dispositivo?", "Ouvrir les paramètres de chiffrement de l’appareil maintenant ?", "Geräteverschlüsselung jetzt öffnen?", "Abrir as configurações de criptografia do dispositivo agora?"],
    ["Open sign-in settings now?", "¿Abrir ahora las opciones de inicio de sesión?", "Ouvrir les options de connexion maintenant ?", "Anmeldeoptionen jetzt öffnen?", "Abrir as opções de entrada agora?"],
    ["[1] Choose what to fix", "[1] Elegir qué corregir", "[1] Choisir quoi corriger", "[1] Auswählen, was ich korrigiere", "[1] Escolher o que corrigir"],
    ["[3] Check my PC again", "[3] Revisar mi PC de nuevo", "[3] Vérifier à nouveau mon PC", "[3] Meinen PC erneut prüfen", "[3] Verificar meu PC novamente"],
    ["Desktop requests require a non-elevated window", "Las solicitudes de escritorio necesitan una ventana sin privilegios de administrador", "Les demandes de bureau nécessitent une fenêtre sans droits administrateur", "Desktopanfragen benötigen ein Fenster ohne Administratorrechte", "As solicitações do desktop precisam de uma janela sem privilégios de administrador"],
    ["That action did not finish. You can view the details before trying again.", "La acción no terminó. Puedes ver los detalles antes de reintentar.", "Cette action n’a pas abouti. Vous pouvez consulter les détails avant de réessayer.", "Diese Aktion wurde nicht abgeschlossen. Du kannst die Details vor einem neuen Versuch ansehen.", "Essa ação não terminou. Você pode ver os detalhes antes de tentar novamente."],
    ["Return to the PC check? Windows will ask for administrator permission again.", "¿Volver a la revisión del PC? Windows volverá a pedir permiso de administrador.", "Revenir à la vérification du PC ? Windows demandera à nouveau l’autorisation administrateur.", "Zur PC-Prüfung zurückkehren? Windows fragt erneut nach Administratorberechtigung.", "Voltar à verificação do PC? O Windows pedirá permissão de administrador novamente."],
    ["To open Settings from here, close Secblitz and open it normally, without Run as administrator.", "Para abrir Configuración desde aquí, cierra Secblitz y ábrelo normalmente, sin Ejecutar como administrador.", "Pour ouvrir les Paramètres depuis ici, fermez Secblitz et ouvrez-le normalement, sans Exécuter en tant qu’administrateur.", "Um die Einstellungen von hier zu öffnen, schließe Secblitz und öffne es normal, ohne Als Administrator ausführen.", "Para abrir Configurações por aqui, feche o Secblitz e abra normalmente, sem Executar como administrador."],
    ["Return to your original Secblitz window to open Settings? That window will ask you again before opening anything.", "¿Volver a tu ventana original de Secblitz para abrir Configuración? Esa ventana te preguntará de nuevo antes de abrir nada.", "Revenir à votre fenêtre Secblitz d’origine pour ouvrir les Paramètres ? Elle vous demandera à nouveau votre accord avant toute ouverture.", "Zum ursprünglichen Secblitz-Fenster zurückkehren, um Einstellungen zu öffnen? Es fragt erneut nach, bevor etwas geöffnet wird.", "Voltar à janela original do Secblitz para abrir Configurações? Ela perguntará novamente antes de abrir qualquer coisa."],
    ["Unexpected action result: ", "Resultado inesperado de la acción: ", "Résultat d’action inattendu : ", "Unerwartetes Aktionsergebnis: ", "Resultado inesperado da ação: "],
    ["Worker panicked without a text payload", "El proceso de trabajo falló sin mensaje de texto", "Le processus de travail s’est interrompu sans message texte", "Arbeitsthread ohne Textmeldung abgebrochen", "O processo de trabalho falhou sem mensagem de texto"],
    ["Action worker failed: ", "Falló el proceso de la acción: ", "Échec du traitement de l’action : ", "Aktionsverarbeitung fehlgeschlagen: ", "Falha no processamento da ação: "],
    ["Extra actions are not part of Undo my last fixes.", "Las acciones adicionales no se incluyen en Deshacer mis últimas correcciones.", "Les actions supplémentaires ne font pas partie d’Annuler mes dernières corrections.", "Zusatzaktionen gehören nicht zu Meine letzten Korrekturen rückgängig machen.", "Ações extras não fazem parte de Desfazer minhas últimas correções."],
    ["Choose Check again when you are ready to verify current protection.", "Elige Revisar mi PC de nuevo cuando quieras comprobar la protección actual.", "Choisissez Vérifier à nouveau mon PC pour vérifier la protection actuelle.", "Wähle Meinen PC erneut prüfen, wenn du den aktuellen Schutz prüfen möchtest.", "Escolha Verificar meu PC novamente quando quiser verificar a proteção atual."],
    ["Select at least one control", "Selecciona al menos una medida", "Sélectionnez au moins une mesure", "Wähle mindestens eine Maßnahme", "Selecione pelo menos uma medida"],
    ["Duplicate selected control: ", "Medida seleccionada más de una vez: ", "Mesure sélectionnée plusieurs fois : ", "Maßnahme mehrfach ausgewählt: ", "Medida selecionada mais de uma vez: "],
    ["Unknown selected control: ", "Medida seleccionada desconocida: ", "Mesure sélectionnée inconnue : ", "Unbekannte ausgewählte Maßnahme: ", "Medida selecionada desconhecida: "],
    ["Duplicate active control owner; revert before applying", "Una medida pertenece a varias transacciones activas; deshaz los cambios antes de aplicar", "Une mesure appartient à plusieurs transactions actives ; annulez avant d’appliquer", "Maßnahme gehört mehreren aktiven Transaktionen; vor dem Anwenden rückgängig machen", "Uma medida pertence a várias transações ativas; desfaça antes de aplicar"],
    ["Selected batch blocked by an owned control conflict or probe failure", "El lote seleccionado está bloqueado por un conflicto en una medida ya registrada o un fallo de comprobación", "Le lot sélectionné est bloqué par un conflit sur une mesure déjà enregistrée ou un échec de vérification", "Ausgewählte Gruppe durch Konflikt einer bereits protokollierten Maßnahme oder Prüfungsfehler blockiert", "O lote selecionado está bloqueado por conflito em uma medida já registrada ou falha de verificação"],
    ["Incomplete transaction precedes another active transaction", "Una transacción incompleta precede a otra transacción activa", "Une transaction incomplète précède une autre transaction active", "Unvollständige Transaktion steht vor einer weiteren aktiven Transaktion", "Uma transação incompleta precede outra transação ativa"],
    ["Duplicate active control owner; journal history is invalid", "Una medida pertenece a varias transacciones activas; el historial del diario no es válido", "Une mesure appartient à plusieurs transactions actives ; l’historique du journal n’est pas valide", "Maßnahme gehört mehreren aktiven Transaktionen; Journalverlauf ist ungültig", "Uma medida pertence a várias transações ativas; o histórico do diário é inválido"],
    ["Unknown settings URI", "URI de configuración desconocido", "URI de paramètres inconnu", "Unbekannte Einstellungs-URI", "URI de configurações desconhecido"],
    ["Open Settings from the non-elevated interactive application", "Abre Configuración desde la aplicación interactiva sin privilegios de administrador", "Ouvrez les Paramètres depuis l’application interactive sans droits administrateur", "Öffne die Einstellungen aus der interaktiven Anwendung ohne erhöhte Rechte", "Abra as Configurações pelo aplicativo interativo sem privilégios elevados"],
    ["Settings actions require Windows", "Las acciones de configuración requieren Windows", "Les actions sur les paramètres nécessitent Windows", "Einstellungsaktionen erfordern Windows", "As ações de configurações exigem Windows"],
    ["Windows could not open settings", "Windows no pudo abrir la configuración", "Windows n’a pas pu ouvrir les paramètres", "Windows konnte die Einstellungen nicht öffnen", "O Windows não conseguiu abrir as configurações"],
    ["Windows accepted the settings-page request. Page availability and security settings are not verified; no fix is claimed.", "Windows aceptó la solicitud de abrir la página de configuración. No se verifica su disponibilidad ni los ajustes de seguridad; no se afirma ninguna corrección.", "Windows a accepté la demande d’ouverture des paramètres. La disponibilité de la page et les réglages de sécurité ne sont pas vérifiés ; aucune correction n’est affirmée.", "Windows hat die Anfrage für die Einstellungsseite angenommen. Seitenverfügbarkeit und Sicherheitseinstellungen sind nicht geprüft; eine Korrektur wird nicht behauptet.", "O Windows aceitou a solicitação da página de configurações. A disponibilidade da página e as configurações de segurança não foram verificadas; nenhuma correção é afirmada."],
    ["Defender's signature-update command returned successfully using its configured sources. This does not establish that signatures are the latest available.", "El comando de actualización de firmas de Defender terminó sin errores usando las fuentes configuradas. Esto no confirma que sean las firmas más recientes disponibles.", "La commande de mise à jour des signatures Defender s’est terminée sans erreur avec les sources configurées. Cela ne confirme pas que les signatures sont les plus récentes disponibles.", "Defenders Signaturupdate-Befehl ist mit den eingerichteten Quellen erfolgreich zurückgekehrt. Das bestätigt nicht, dass die neuesten verfügbaren Signaturen vorliegen.", "O comando de atualização de assinaturas do Defender retornou sem erro usando as fontes configuradas. Isso não confirma que as assinaturas sejam as mais recentes disponíveis."],
    ["Defender's quick-scan command returned successfully. Completion and threat status are not independently verified; review Windows Security for results.", "El comando de análisis rápido de Defender terminó sin errores. No se verifican de forma independiente la finalización ni el estado de amenazas; consulta los resultados en Seguridad de Windows.", "La commande d’analyse rapide Defender s’est terminée sans erreur. La fin de l’analyse et l’état des menaces ne sont pas vérifiés indépendamment ; consultez les résultats dans Sécurité Windows.", "Defenders Schnellüberprüfungsbefehl ist erfolgreich zurückgekehrt. Abschluss und Bedrohungsstatus sind nicht unabhängig geprüft; Ergebnisse in Windows-Sicherheit prüfen.", "O comando de verificação rápida do Defender retornou sem erro. A conclusão e o estado das ameaças não foram verificados independentemente; consulte os resultados na Segurança do Windows."],
    ["Monitor startup failed; the installed service was retained. Check service status before retrying", "El inicio del monitor falló; se conservó el servicio instalado. Comprueba su estado antes de reintentar", "Le démarrage du moniteur a échoué ; le service installé a été conservé. Vérifiez son état avant de réessayer", "Monitorstart fehlgeschlagen; installierter Dienst wurde beibehalten. Dienststatus vor erneutem Versuch prüfen", "A inicialização do monitor falhou; o serviço instalado foi mantido. Verifique seu estado antes de tentar novamente"],
    ["SCM reports SecblitzMonitor Running. Monitoring is read-only; this does not verify report freshness or machine health.", "SCM informa que SecblitzMonitor está en ejecución. La supervisión es de solo lectura; esto no verifica la actualidad de los informes ni el estado del equipo.", "SCM indique que SecblitzMonitor est en cours d’exécution. La surveillance est en lecture seule ; cela ne vérifie ni l’actualité des rapports ni l’état du PC.", "SCM meldet SecblitzMonitor als laufend. Die Überwachung liest nur; dies bestätigt weder aktuelle Berichte noch den Zustand des Computers.", "O SCM informa que SecblitzMonitor está em execução. O monitoramento é somente leitura; isso não verifica a atualidade dos relatórios nem a integridade do computador."],
    ["Defender support actions require Windows", "Las acciones de mantenimiento de Defender requieren Windows", "Les actions de maintenance Defender nécessitent Windows", "Defender-Wartungsaktionen erfordern Windows", "As ações de manutenção do Defender exigem Windows"],
    ["Unknown support action id", "Identificador de acción de mantenimiento desconocido", "Identifiant d’action de maintenance inconnu", "Unbekannte Kennung der Wartungsaktion", "Identificador de ação de manutenção desconhecido"],
    ["Embedded backend dispatcher boundary changed", "Cambió el límite del despachador del motor integrado", "La limite du répartiteur intégré du moteur a changé", "Grenze des eingebetteten Backend-Dispatchers geändert", "O limite do despachante do mecanismo incorporado mudou"],
    ["Ambiguous embedded backend dispatcher boundary", "Límite ambiguo del despachador del motor integrado", "Limite ambiguë du répartiteur intégré du moteur", "Mehrdeutige Grenze des eingebetteten Backend-Dispatchers", "Limite ambíguo do despachante do mecanismo incorporado"],
    ["Defender support actions require Administrator elevation", "Las acciones de mantenimiento de Defender requieren privilegios de administrador", "Les actions de maintenance Defender nécessitent les droits administrateur", "Defender-Wartungsaktionen erfordern Administratorrechte", "As ações de manutenção do Defender exigem privilégios de administrador"],
    ["Defender support action failed; work may continue in Defender. Review Windows Security; no completion or rollback is assumed", "La acción de mantenimiento falló; Defender puede seguir trabajando. Revisa Seguridad de Windows; no se supone finalización ni reversión", "L’action de maintenance a échoué ; Defender peut continuer à travailler. Consultez Sécurité Windows ; ni fin ni annulation ne sont présumées", "Defender-Wartungsaktion fehlgeschlagen; Defender arbeitet möglicherweise weiter. Windows-Sicherheit prüfen; weder Abschluss noch Rücknahme wird angenommen", "A ação de manutenção falhou; o Defender pode continuar trabalhando. Consulte Segurança do Windows; não se presume conclusão nem reversão"],
    ["Defender command return was not acknowledged; review Windows Security", "No se confirmó el retorno del comando de Defender; consulta Seguridad de Windows", "Le retour de la commande Defender n’a pas été confirmé ; consultez Sécurité Windows", "Rückkehr des Defender-Befehls wurde nicht bestätigt; Windows-Sicherheit prüfen", "O retorno do comando do Defender não foi confirmado; consulte Segurança do Windows"],
    ["Service startup requires Administrator elevation", "El inicio del servicio requiere privilegios de administrador", "Le démarrage du service nécessite les droits administrateur", "Dienststart erfordert Administratorrechte", "A inicialização do serviço exige privilégios de administrador"],
    ["Cannot inspect service security", "No se puede examinar la seguridad del servicio", "Impossible d’examiner la sécurité du service", "Dienstsicherheit kann nicht geprüft werden", "Não foi possível examinar a segurança do serviço"],
    ["Monitor cannot start from SCM state ", "El monitor no puede iniciarse desde el estado SCM ", "Le moniteur ne peut pas démarrer depuis l’état SCM ", "Monitor kann nicht aus folgendem SCM-Status starten: ", "O monitor não pode iniciar a partir do estado SCM "],
    ["Monitor did not reach Running: ", "El monitor no pasó al estado en ejecución: ", "Le moniteur n’a pas atteint l’état en cours d’exécution : ", "Monitor hat den Status Wird ausgeführt nicht erreicht: ", "O monitor não atingiu o estado em execução: "],
    ["Timed out waiting for SCM Running; the monitor may still start. Installation was retained", "Se agotó el tiempo esperando el estado en ejecución de SCM; el monitor aún podría iniciarse. Se conservó la instalación", "Délai dépassé en attendant l’état en cours d’exécution de SCM ; le moniteur peut encore démarrer. L’installation a été conservée", "Zeitlimit beim Warten auf SCM-Status Wird ausgeführt überschritten; Monitor kann noch starten. Installation beibehalten", "Tempo esgotado aguardando o estado em execução do SCM; o monitor ainda pode iniciar. A instalação foi mantida"],
    ["Unexpected service configuration; refusing to start", "Configuración del servicio inesperada; se rechaza el inicio", "Configuration du service inattendue ; démarrage refusé", "Unerwartete Dienstkonfiguration; Start verweigert", "Configuração inesperada do serviço; inicialização recusada"],
    ["Cannot inspect service owner", "No se puede examinar el propietario del servicio", "Impossible de vérifier le propriétaire du service", "Diensteigentümer kann nicht geprüft werden", "Não foi possível verificar o proprietário do serviço"],
    ["Untrusted service owner", "Propietario del servicio no confiable", "Propriétaire du service non fiable", "Nicht vertrauenswürdiger Diensteigentümer", "Proprietário do serviço não confiável"],
    ["Missing or invalid service DACL", "DACL del servicio ausente o no válida", "DACL du service absente ou non valide", "Fehlende oder ungültige Dienst-DACL", "DACL do serviço ausente ou inválida"],
    ["Unprotected service DACL", "DACL del servicio sin protección", "DACL du service non protégée", "Ungeschützte Dienst-DACL", "DACL do serviço desprotegida"],
    ["Cannot inspect service ACE", "No se puede examinar la ACE del servicio", "Impossible d’examiner l’ACE du service", "Dienst-ACE kann nicht geprüft werden", "Não foi possível examinar a ACE do serviço"],
    ["Unexpected service ACE", "ACE del servicio inesperada", "ACE du service inattendue", "Unerwartete Dienst-ACE", "ACE do serviço inesperada"],
    ["Invalid service trustee", "Destinatario de permisos del servicio no válido", "Bénéficiaire des autorisations du service non valide", "Ungültiger Dienstberechtigter", "Destinatário de permissões do serviço inválido"],
    ["Unexpected service trustee", "Destinatario de permisos del servicio inesperado", "Bénéficiaire des autorisations du service inattendu", "Unerwarteter Dienstberechtigter", "Destinatário de permissões do serviço inesperado"],
    ["Unexpected service permissions", "Permisos del servicio inesperados", "Autorisations du service inattendues", "Unerwartete Dienstberechtigungen", "Permissões do serviço inesperadas"],
    ["Missing service trustees", "Faltan destinatarios de permisos del servicio", "Bénéficiaires des autorisations du service manquants", "Dienstberechtigte fehlen", "Destinatários de permissões do serviço ausentes"],
    ["Unknown support operation", "Operación de mantenimiento desconocida", "Opération de maintenance inconnue", "Unbekannter Wartungsvorgang", "Operação de manutenção desconhecida"],
    ["Domain-managed or unknown membership: support action declined", "Administrado por dominio o pertenencia desconocida: acción de mantenimiento rechazada", "Gestion par domaine ou appartenance inconnue : action de maintenance refusée", "Domänenverwaltung oder unbekannte Mitgliedschaft: Wartungsaktion abgelehnt", "Gerenciado por domínio ou associação desconhecida: ação de manutenção recusada"],
    ["MDM-managed device: support action declined", "Dispositivo administrado por MDM: acción de mantenimiento rechazada", "Appareil géré par MDM : action de maintenance refusée", "MDM-verwaltetes Gerät: Wartungsaktion abgelehnt", "Dispositivo gerenciado por MDM: ação de manutenção recusada"],
    ["Enrollment or cloud-management evidence: support action declined", "Indicios de inscripción o administración en la nube: acción de mantenimiento rechazada", "Indices d’inscription ou de gestion cloud : action de maintenance refusée", "Hinweise auf Registrierung oder Cloudverwaltung: Wartungsaktion abgelehnt", "Indícios de inscrição ou gerenciamento em nuvem: ação de manutenção recusada"],
    ["Local policy artifacts: support action declined", "Indicios de directiva local: acción de mantenimiento rechazada", "Traces de stratégie locale : action de maintenance refusée", "Lokale Richtlinienartefakte: Wartungsaktion abgelehnt", "Indícios de política local: ação de manutenção recusada"],
    ["Additional, missing or unrecognized antivirus provider: support action declined", "Proveedor antivirus adicional, ausente o desconocido: acción de mantenimiento rechazada", "Fournisseur antivirus supplémentaire, absent ou inconnu : action de maintenance refusée", "Zusätzlicher, fehlender oder unbekannter Antivirusanbieter: Wartungsaktion abgelehnt", "Provedor antivírus adicional, ausente ou desconhecido: ação de manutenção recusada"],
    ["Defender is not confirmed active in Normal mode", "No se ha confirmado que Defender esté activo en modo Normal", "Defender n’est pas confirmé actif en mode Normal", "Defender ist nicht als aktiv im Modus Normal bestätigt", "Não foi confirmado que o Defender esteja ativo no modo Normal"],
    ["Install Bitwarden in your original desktop account now?", "¿Instalar Bitwarden ahora en tu cuenta de escritorio original?", "Installer Bitwarden maintenant dans votre compte de bureau d’origine ?", "Bitwarden jetzt in deinem ursprünglichen Desktopkonto installieren?", "Instalar o Bitwarden agora na sua conta original do desktop?"],
    ["Bitwarden installation did not finish. Review the failure before trying again.", "La instalación de Bitwarden no terminó. Revisa el error antes de reintentar.", "L’installation de Bitwarden n’a pas abouti. Examinez l’erreur avant de réessayer.", "Die Bitwarden-Installation wurde nicht abgeschlossen. Prüfe den Fehler vor einem neuen Versuch.", "A instalação do Bitwarden não terminou. Revise a falha antes de tentar novamente."],
    ["Show technical details of this failure?", "¿Mostrar los detalles técnicos de este error?", "Afficher les détails techniques de cette erreur ?", "Technische Details dieses Fehlers anzeigen?", "Mostrar os detalhes técnicos desta falha?"],
    ["A safer PC. Without headaches.", "Un PC más seguro. Sin dolores de cabeza.", "Un PC plus sûr. Sans prise de tête.", "Ein sichererer PC. Ohne Kopfzerbrechen.", "Um PC mais seguro. Sem dor de cabeça."],
    ["No command: open the guided security check. Nothing is fixed without your choice.", "Sin comando: abre la revisión guiada de seguridad. Tú decides qué se corrige.", "Sans commande : ouvre la vérification de sécurité guidée. Vous choisissez ce qui sera corrigé.", "Ohne Befehl: öffnet die geführte Sicherheitsprüfung. Du entscheidest, was korrigiert wird.", "Sem comando: abre a verificação guiada de segurança. Você escolhe o que corrigir."],
    ["Show technical report details", "Mostrar los detalles técnicos del informe", "Afficher les détails techniques du rapport", "Technische Berichtsdetails anzeigen", "Mostrar detalhes técnicos do relatório"],
    ["Guided security check and selected fixes", "Revisión guiada y correcciones a tu elección", "Vérification guidée et corrections à votre choix", "Geführte Prüfung und Korrekturen nach deiner Wahl", "Verificação guiada e correções à sua escolha"],
    ["Start the optional monitoring service", "Iniciar el servicio opcional de supervisión", "Démarrer le service de surveillance facultatif", "Optionalen Überwachungsdienst starten", "Iniciar o serviço opcional de monitoramento"],
    ["Monitoring is running. Check reports separately to verify their freshness.", "La supervisión está en marcha. Consulta los informes para comprobar si están actualizados.", "La surveillance est en cours. Consultez les rapports pour vérifier leur actualité.", "Die Überwachung läuft. Prüfe die Berichte, um sicherzugehen, dass sie aktuell sind.", "O monitoramento está ativo. Consulte os relatórios para verificar se estão atualizados."],
    ["The operation could not be completed. Run again with --details to see technical information.", "No se pudo completar la operación. Vuelve a ejecutar con --details para ver la información técnica.", "L’opération n’a pas pu être terminée. Relancez avec --details pour voir les informations techniques.", "Der Vorgang konnte nicht abgeschlossen werden. Starte ihn mit --details erneut, um technische Informationen zu sehen.", "Não foi possível concluir a operação. Execute novamente com --details para ver as informações técnicas."],
    ["Saved changes are available for review or undo. This does not mean every requested fix completed.", "Puedes revisar o deshacer los cambios guardados. Esto no significa que se completaran todas las correcciones solicitadas.", "Vous pouvez examiner ou annuler les modifications enregistrées. Cela ne signifie pas que toutes les corrections demandées ont abouti.", "Gespeicherte Änderungen kannst du prüfen oder rückgängig machen. Das bedeutet nicht, dass alle angeforderten Korrekturen abgeschlossen wurden.", "Você pode revisar ou desfazer as alterações salvas. Isso não significa que todas as correções solicitadas foram concluídas."],
    ["The guided check needs an interactive terminal. Open a terminal and run secblitz guide, or use secblitz audit --json for a report.", "La revisión guiada necesita una terminal interactiva. Abre una terminal y ejecuta secblitz guide, o usa secblitz audit --json para obtener un informe.", "La vérification guidée nécessite un terminal interactif. Ouvrez un terminal et lancez secblitz guide, ou utilisez secblitz audit --json pour obtenir un rapport.", "Die geführte Prüfung benötigt ein interaktives Terminal. Öffne ein Terminal und starte secblitz guide oder nutze secblitz audit --json für einen Bericht.", "A verificação guiada precisa de um terminal interativo. Abra um terminal e execute secblitz guide, ou use secblitz audit --json para obter um relatório."],
    ["[1] Yes  [0] No (default)", "[1] Sí  [0] No (predeterminado)", "[1] Oui  [0] Non (par défaut)", "[1] Ja  [0] Nein (Standard)", "[1] Sim  [0] Não (padrão)"],
    ["First, we will check your protection. You choose what to fix; checking does not apply fixes.", "Primero revisaremos tu protección. Tú eliges qué corregir; la revisión no aplica cambios.", "Vérifions d’abord votre protection. Vous choisissez quoi corriger ; la vérification n’applique aucune correction.", "Zuerst prüfen wir deinen Schutz. Du wählst die Korrekturen; die Prüfung selbst ändert nichts.", "Primeiro, vamos verificar sua proteção. Você escolhe o que corrigir; a verificação não aplica correções."],
    ["[1] Fix selected recommended items", "[1] Corregir lo que yo elija", "[1] Corriger ce que je choisis", "[1] Meine Auswahl korrigieren", "[1] Corrigir o que eu escolher"],
    ["[2] Review protection and next steps", "[2] Ver mi protección y los próximos pasos", "[2] Voir ma protection et les prochaines étapes", "[2] Schutz und nächste Schritte ansehen", "[2] Ver minha proteção e os próximos passos"],
    ["[3] Check again", "[3] Revisar mi PC de nuevo", "[3] Vérifier à nouveau mon PC", "[3] Meinen PC erneut prüfen", "[3] Verificar meu PC novamente"],
    ["[4] Undo my last fixes", "[4] Deshacer mis últimas correcciones", "[4] Annuler mes dernières corrections", "[4] Meine letzten Korrekturen rückgängig machen", "[4] Desfazer minhas últimas correções"],
    ["[5] Extra tools", "[5] Más herramientas", "[5] Outils supplémentaires", "[5] Weitere Werkzeuge", "[5] Mais ferramentas"],
    ["[6] Technical details (optional)", "[6] Detalles técnicos (opcional)", "[6] Détails techniques (facultatif)", "[6] Technische Details (optional)", "[6] Detalhes técnicos (opcional)"],
    ["[0] Exit", "[0] Salir", "[0] Quitter", "[0] Beenden", "[0] Sair"],
    ["Check again before choosing fixes. The previous check is no longer current.", "Vuelve a revisar antes de elegir correcciones. La revisión anterior ya no está actualizada.", "Relancez la vérification avant de choisir des corrections. La précédente n’est plus à jour.", "Prüfe erneut, bevor du Korrekturen auswählst. Die vorherige Prüfung ist nicht mehr aktuell.", "Verifique novamente antes de escolher correções. A verificação anterior não está mais atualizada."],
    ["There are no recommended automatic fixes available. See details for other next steps.", "No hay correcciones automáticas recomendadas disponibles. Consulta los detalles para ver qué más puedes hacer.", "Aucune correction automatique recommandée n’est disponible. Consultez les détails pour les autres étapes possibles.", "Es sind keine empfohlenen automatischen Korrekturen verfügbar. In den Details findest du weitere Schritte.", "Não há correções automáticas recomendadas disponíveis. Veja os detalhes para saber o que mais fazer."],
    ["Choose only the items you want to fix:", "Elige solo lo que quieras corregir:", "Choisissez uniquement ce que vous voulez corriger :", "Wähle nur aus, was du korrigieren möchtest:", "Escolha apenas o que deseja corrigir:"],
    ["all", "todos", "tout", "alle", "todos"],
    ["none", "ninguno", "aucun", "keine", "nenhum"],
    ["complete", "completado", "terminé", "abgeschlossen", "concluído"],
    ["opened", "abierto", "ouvert", "geöffnet", "aberto"],
    ["returned", "comando terminado", "commande terminée", "Befehl zurückgekehrt", "comando retornou"],
    ["running", "en marcha", "en cours", "läuft", "em execução"],
    ["Enter numbers separated by commas, ranges such as 1-3, all, or none. Enter alone selects nothing.", "Escribe números separados por comas, intervalos como 1-3, todos o ninguno. Pulsar Intro sin escribir no selecciona nada.", "Saisissez des nombres séparés par des virgules, des plages comme 1-3, tout ou aucun. Appuyer seulement sur Entrée ne sélectionne rien.", "Gib durch Kommas getrennte Zahlen, Bereiche wie 1-3, alle oder keine ein. Nur die Eingabetaste wählt nichts aus.", "Digite números separados por vírgulas, intervalos como 1-3, todos ou nenhum. Só pressionar Enter não seleciona nada."],
    ["That selection is not valid. Use only the displayed numbers, all, or none.", "La selección no es válida. Usa solo los números mostrados, todos o ninguno.", "Cette sélection n’est pas valide. Utilisez seulement les nombres affichés, tout ou aucun.", "Diese Auswahl ist ungültig. Nutze nur die angezeigten Zahlen, alle oder keine.", "A seleção não é válida. Use apenas os números exibidos, todos ou nenhum."],
    ["Nothing selected. No changes made.", "No has seleccionado nada. No se hizo ningún cambio.", "Aucune sélection. Aucune modification effectuée.", "Nichts ausgewählt. Nichts geändert.", "Nada selecionado. Nenhuma alteração feita."],
    ["Review your selected fixes:", "Revisa las correcciones que has elegido:", "Vérifiez les corrections que vous avez choisies :", "Prüfe deine ausgewählten Korrekturen:", "Revise as correções que você escolheu:"],
    ["Apply these selected fixes now? Some changes may need a restart.", "¿Aplicar ahora estas correcciones? Algunos cambios pueden necesitar un reinicio.", "Appliquer ces corrections maintenant ? Certaines modifications peuvent nécessiter un redémarrage.", "Diese ausgewählten Korrekturen jetzt anwenden? Manche Änderungen benötigen eventuell einen Neustart.", "Aplicar estas correções agora? Algumas alterações podem exigir reinicialização."],
    ["Check again to review current protection, or undo your recorded fixes.", "Vuelve a revisar la protección actual o deshaz tus correcciones registradas.", "Relancez la vérification de la protection actuelle ou annulez vos corrections enregistrées.", "Prüfe den aktuellen Schutz erneut oder mache deine protokollierten Korrekturen rückgängig.", "Verifique novamente a proteção atual ou desfaça suas correções registradas."],
    ["Undo the latest recorded fixes? This restores their saved original settings; extra tools and software installs are not undone.", "¿Deshacer las últimas correcciones registradas? Se restauran sus ajustes originales guardados; no se deshacen las herramientas adicionales ni las instalaciones de software.", "Annuler les dernières corrections enregistrées ? Leurs réglages d’origine seront rétablis ; les outils supplémentaires et installations de logiciels ne seront pas annulés.", "Die letzten protokollierten Korrekturen rückgängig machen? Ihre gespeicherten ursprünglichen Einstellungen werden wiederhergestellt; zusätzliche Werkzeuge und Softwareinstallationen werden nicht rückgängig gemacht.", "Desfazer as últimas correções registradas? As configurações originais salvas serão restauradas; ferramentas extras e instalações de software não serão desfeitas."],
    ["Technical details of the last failure (may include system paths and native messages):", "Detalles técnicos del último fallo (pueden incluir rutas del sistema y mensajes nativos):", "Détails techniques du dernier échec (peuvent inclure des chemins système et des messages natifs) :", "Technische Details des letzten Fehlers (können Systempfade und native Meldungen enthalten):", "Detalhes técnicos da última falha (podem incluir caminhos do sistema e mensagens nativas):"],
    ["Technical details of the last completed operation (not a new protection check):", "Detalles técnicos de la última operación completada (no es una nueva revisión de protección):", "Détails techniques de la dernière opération terminée (pas une nouvelle vérification de la protection) :", "Technische Details des letzten abgeschlossenen Vorgangs (keine neue Schutzprüfung):", "Detalhes técnicos da última operação concluída (não é uma nova verificação de proteção):"],
    ["Choose one of the displayed menu numbers.", "Elige uno de los números del menú.", "Choisissez un des numéros du menu.", "Wähle eine der angezeigten Menünummern.", "Escolha um dos números do menu."],
    ["The operation did not finish. Some changes may already have been made; remaining work is not confirmed. You can check again or undo recorded fixes.", "La operación no terminó. Puede que ya se hayan hecho algunos cambios; no se confirma el resto. Puedes revisar de nuevo o deshacer las correcciones registradas.", "L’opération n’a pas abouti. Certaines modifications ont peut-être déjà été faites ; le reste n’est pas confirmé. Vous pouvez vérifier à nouveau ou annuler les corrections enregistrées.", "Der Vorgang wurde nicht abgeschlossen. Einige Änderungen können bereits erfolgt sein; die übrigen sind nicht bestätigt. Du kannst erneut prüfen oder protokollierte Korrekturen rückgängig machen.", "A operação não terminou. Algumas alterações podem já ter sido feitas; o restante não está confirmado. Você pode verificar novamente ou desfazer as correções registradas."],
    ["Choose Technical details to see the original failure.", "Elige Detalles técnicos para ver el fallo original.", "Choisissez Détails techniques pour voir l’échec d’origine.", "Wähle Technische Details, um den ursprünglichen Fehler zu sehen.", "Escolha Detalhes técnicos para ver a falha original."],
    ["[1] Generate a password", "[1] Generar una contraseña", "[1] Générer un mot de passe", "[1] Ein Passwort erzeugen", "[1] Gerar uma senha"],
    ["[2] Install Bitwarden (optional password manager)", "[2] Instalar Bitwarden (gestor de contraseñas opcional)", "[2] Installer Bitwarden (gestionnaire de mots de passe facultatif)", "[2] Bitwarden installieren (optionaler Passwortmanager)", "[2] Instalar Bitwarden (gerenciador de senhas opcional)"],
    ["[3] Install and start optional monitoring", "[3] Instalar e iniciar la monitorización opcional", "[3] Installer et démarrer la surveillance facultative", "[3] Optionale Überwachung installieren und starten", "[3] Instalar e iniciar o monitoramento opcional"],
    ["[4] Update Microsoft Defender protection", "[4] Actualizar la protección de Microsoft Defender", "[4] Mettre à jour la protection Microsoft Defender", "[4] Microsoft-Defender-Schutz aktualisieren", "[4] Atualizar a proteção do Microsoft Defender"],
    ["[5] Run a Microsoft Defender quick scan", "[5] Ejecutar un análisis rápido de Microsoft Defender", "[5] Lancer une analyse rapide Microsoft Defender", "[5] Microsoft-Defender-Schnellüberprüfung starten", "[5] Executar uma verificação rápida do Microsoft Defender"],
    ["[0] Back", "[0] Volver", "[0] Retour", "[0] Zurück", "[0] Voltar"],
    ["Return to your original non-administrator window to install Bitwarden? That window will ask for consent again.", "¿Volver a tu ventana original sin privilegios de administrador para instalar Bitwarden? Esa ventana volverá a pedir tu consentimiento.", "Revenir à votre fenêtre d’origine sans droits administrateur pour installer Bitwarden ? Elle vous demandera à nouveau votre accord.", "Zum ursprünglichen Fenster ohne Administratorrechte zurückkehren, um Bitwarden zu installieren? Dort wirst du erneut um Zustimmung gebeten.", "Voltar à janela original sem privilégios de administrador para instalar o Bitwarden? Ela pedirá seu consentimento novamente."],
    ["Bitwarden must be installed from your normal, non-administrator desktop terminal. Open that terminal and run secblitz tools bitwarden --yes after reviewing the installation consent in --help.", "Bitwarden debe instalarse desde tu terminal habitual sin privilegios de administrador. Ábrelo y ejecuta secblitz tools bitwarden --yes tras leer el consentimiento de instalación en --help.", "Bitwarden doit être installé depuis votre terminal habituel sans droits administrateur. Ouvrez-le et lancez secblitz tools bitwarden --yes après avoir lu les conditions de consentement dans --help.", "Bitwarden muss in deinem normalen Desktopterminal ohne Administratorrechte installiert werden. Öffne es und führe secblitz tools bitwarden --yes aus, nachdem du die Zustimmungshinweise in --help gelesen hast.", "O Bitwarden deve ser instalado pelo terminal normal do desktop, sem privilégios de administrador. Abra esse terminal e execute secblitz tools bitwarden --yes depois de ler o consentimento de instalação em --help."],
    ["Opening settings does not fix a finding. Follow the Windows instructions, then check again.", "Abrir la configuración no corrige el problema. Sigue las instrucciones de Windows y vuelve a comprobar.", "Ouvrir les paramètres ne corrige pas le problème. Suivez les instructions de Windows, puis vérifiez à nouveau.", "Das Öffnen der Einstellungen behebt keinen Befund. Folge den Windows-Anweisungen und prüfe danach erneut.", "Abrir as configurações não corrige o problema. Siga as instruções do Windows e verifique novamente."],
    ["[1] Open Windows Update settings", "[1] Abrir la configuración de Windows Update", "[1] Ouvrir les paramètres de Windows Update", "[1] Windows-Update-Einstellungen öffnen", "[1] Abrir as configurações do Windows Update"],
    ["[2] Open Windows Security settings", "[2] Abrir la configuración de Seguridad de Windows", "[2] Ouvrir les paramètres de Sécurité Windows", "[2] Windows-Sicherheit öffnen", "[2] Abrir as configurações de Segurança do Windows"],
    ["[3] Open device encryption / BitLocker settings", "[3] Abrir la configuración de cifrado del dispositivo / BitLocker", "[3] Ouvrir les paramètres de chiffrement de l’appareil / BitLocker", "[3] Geräteverschlüsselung / BitLocker öffnen", "[3] Abrir as configurações de criptografia do dispositivo / BitLocker"],
    ["[4] Open sign-in settings", "[4] Abrir las opciones de inicio de sesión", "[4] Ouvrir les options de connexion", "[4] Anmeldeoptionen öffnen", "[4] Abrir as opções de entrada"],
    ["[5] Update Microsoft Defender protection", "[5] Actualizar la protección de Microsoft Defender", "[5] Mettre à jour la protection Microsoft Defender", "[5] Microsoft-Defender-Schutz aktualisieren", "[5] Atualizar a proteção do Microsoft Defender"],
    ["[6] Run a Microsoft Defender quick scan", "[6] Ejecutar un análisis rápido de Microsoft Defender", "[6] Lancer une analyse rapide Microsoft Defender", "[6] Microsoft-Defender-Schnellüberprüfung starten", "[6] Executar uma verificação rápida do Microsoft Defender"],
    ["Defender will connect to its configured update sources and download protection updates.", "Defender se conectará a sus fuentes de actualización configuradas y descargará actualizaciones de protección.", "Defender se connectera à ses sources de mise à jour configurées et téléchargera les mises à jour de protection.", "Defender verbindet sich mit seinen eingerichteten Updatequellen und lädt Schutzupdates herunter.", "O Defender se conectará às fontes de atualização configuradas e baixará atualizações de proteção."],
    ["Defender will scan your device and may remediate threats using your existing Defender settings. This can take several minutes.", "Defender analizará el dispositivo y podrá corregir amenazas según tu configuración actual. Puede tardar varios minutos.", "Defender analysera votre appareil et pourra traiter les menaces selon vos réglages actuels. Cela peut prendre plusieurs minutes.", "Defender überprüft dein Gerät und kann Bedrohungen gemäß deinen vorhandenen Einstellungen beheben. Das kann mehrere Minuten dauern.", "O Defender verificará seu dispositivo e poderá tratar ameaças conforme suas configurações atuais. Isso pode levar vários minutos."],
    ["This installs the optional read-only monitor if needed and starts it. It does not automatically fix findings.", "Esto instala el monitor opcional de solo lectura si hace falta y lo inicia. No corrige automáticamente los problemas detectados.", "Cette action installe si nécessaire le moniteur facultatif en lecture seule et le démarre. Il ne corrige pas automatiquement les problèmes détectés.", "Dies installiert bei Bedarf den optionalen Monitor mit reinem Lesezugriff und startet ihn. Er korrigiert Befunde nicht automatisch.", "Isso instala o monitor opcional de somente leitura, se necessário, e o inicia. Ele não corrige automaticamente os problemas encontrados."],
    ["Open settings only: no fix is applied or verified by opening this page.", "Solo abre la configuración: abrir esta página no aplica ni verifica ninguna corrección.", "Ouvre uniquement les paramètres : aucune correction n’est appliquée ni vérifiée en ouvrant cette page.", "Öffnet nur die Einstellungen: Dadurch wird keine Korrektur angewendet oder bestätigt.", "Apenas abre as configurações: abrir esta página não aplica nem verifica nenhuma correção."],
    ["Run the selected extra action now? Extra actions are not part of Undo my last fixes.", "¿Ejecutar ahora la acción adicional elegida? Estas acciones no se incluyen en Deshacer mis últimas mejoras.", "Exécuter l’action supplémentaire choisie maintenant ? Ces actions ne font pas partie d’Annuler mes dernières corrections.", "Die ausgewählte Zusatzaktion jetzt ausführen? Zusatzaktionen gehören nicht zu Meine letzten Korrekturen rückgängig machen.", "Executar a ação extra escolhida agora? Ações extras não fazem parte de Desfazer minhas últimas correções."],
    ["Settings opened. Follow the Windows instructions; opening settings does not mean the issue is fixed.", "Configuración abierta. Sigue las instrucciones de Windows; abrirla no significa que el problema esté resuelto.", "Paramètres ouverts. Suivez les instructions de Windows ; leur ouverture ne signifie pas que le problème est résolu.", "Einstellungen geöffnet. Folge den Windows-Anweisungen; das Öffnen bedeutet nicht, dass das Problem behoben ist.", "Configurações abertas. Siga as instruções do Windows; abrir as configurações não significa que o problema foi resolvido."],
    ["Defender's command returned. Review Windows Security for update or scan results, then check again.", "El comando de Defender finalizó. Consulta los resultados de la actualización o del análisis en Seguridad de Windows y vuelve a comprobar.", "La commande Defender s’est terminée. Consultez Sécurité Windows pour les résultats de mise à jour ou d’analyse, puis vérifiez à nouveau.", "Der Defender-Befehl ist zurückgekehrt. Prüfe Update- oder Scanergebnisse in Windows-Sicherheit und prüfe danach erneut.", "O comando do Defender retornou. Veja os resultados da atualização ou verificação na Segurança do Windows e verifique novamente."],
    ["The action could not be verified. Defender work may still be running. Review Windows Security or service status before trying again.", "No se pudo verificar la acción. Defender puede seguir trabajando. Revisa Seguridad de Windows o el estado del servicio antes de reintentar.", "L’action n’a pas pu être vérifiée. Defender peut encore être en cours d’exécution. Consultez Sécurité Windows ou l’état du service avant de réessayer.", "Die Aktion konnte nicht bestätigt werden. Defender arbeitet möglicherweise noch. Prüfe Windows-Sicherheit oder den Dienststatus, bevor du es erneut versuchst.", "Não foi possível verificar a ação. O Defender pode ainda estar trabalhando. Consulte Segurança do Windows ou o estado do serviço antes de tentar novamente."],
    ["Open Windows Security and review Virus & threat protection.", "Abre Seguridad de Windows y revisa Protección antivirus y contra amenazas.", "Ouvrez Sécurité Windows et consultez Protection contre les virus et menaces.", "Öffne Windows-Sicherheit und prüfe Viren- & Bedrohungsschutz.", "Abra Segurança do Windows e revise Proteção contra vírus e ameaças."],
    ["Open Windows Security and review Firewall & network protection.", "Abre Seguridad de Windows y revisa Firewall y protección de red.", "Ouvrez Sécurité Windows et consultez Pare-feu et protection du réseau.", "Öffne Windows-Sicherheit und prüfe Firewall- & Netzwerkschutz.", "Abra Segurança do Windows e revise Firewall e proteção de rede."],
    ["Review User Account Control settings with your administrator.", "Revisa la configuración del Control de cuentas de usuario con tu administrador.", "Examinez les paramètres du contrôle de compte d’utilisateur avec votre administrateur.", "Prüfe die Benutzerkontensteuerung mit deinem Administrator.", "Revise as configurações do Controle de Conta de Usuário com seu administrador."],
    ["Ask your administrator to review app installation permissions.", "Pide a tu administrador que revise los permisos para instalar aplicaciones.", "Demandez à votre administrateur de vérifier les autorisations d’installation des applications.", "Bitte deinen Administrator, die Berechtigungen zur App-Installation zu prüfen.", "Peça ao administrador para revisar as permissões de instalação de aplicativos."],
    ["Ask your administrator to review anonymous access to account names.", "Pide a tu administrador que revise el acceso anónimo a los nombres de cuenta.", "Demandez à votre administrateur de vérifier l’accès anonyme aux noms de compte.", "Bitte deinen Administrator, den anonymen Zugriff auf Kontonamen zu prüfen.", "Peça ao administrador para revisar o acesso anônimo aos nomes de conta."],
    ["Review account passwords and remote sign-in access with your administrator.", "Revisa las contraseñas de las cuentas y el acceso remoto con tu administrador.", "Examinez les mots de passe des comptes et les connexions à distance avec votre administrateur.", "Prüfe Kontopasswörter und Fernanmeldungen mit deinem Administrator.", "Revise as senhas das contas e o acesso remoto com seu administrador."],
    ["Ask your administrator to review how Windows keeps sign-in secrets.", "Pide a tu administrador que revise cómo guarda Windows los datos de acceso.", "Demandez à votre administrateur de vérifier comment Windows conserve les secrets de connexion.", "Bitte deinen Administrator zu prüfen, wie Windows Anmeldedaten speichert.", "Peça ao administrador para revisar como o Windows guarda os dados de acesso."],
    ["Ask your administrator to review update service permissions.", "Pide a tu administrador que revise los permisos de los servicios de actualización.", "Demandez à votre administrateur de vérifier les autorisations des services de mise à jour.", "Bitte deinen Administrator, die Berechtigungen der Updatedienste zu prüfen.", "Peça ao administrador para revisar as permissões dos serviços de atualização."],
    ["View details and run the check again before deciding what to change.", "Consulta los detalles y repite la comprobación antes de decidir qué cambiar.", "Consultez les détails et relancez la vérification avant de décider quoi modifier.", "Sieh dir die Details an und prüfe erneut, bevor du Änderungen auswählst.", "Veja os detalhes e verifique novamente antes de decidir o que mudar."],
    ["Secblitz can fix this. Help protect against uninvited connections.", "Secblitz puede corregirlo. Ayuda a bloquear conexiones no deseadas.", "Secblitz peut corriger ce réglage pour mieux vous protéger des connexions indésirables.", "Secblitz kann das korrigieren und vor unerwünschten Verbindungen schützen.", "O Secblitz pode corrigir isso e ajudar a proteger contra conexões indesejadas."],
    ["Secblitz can fix this. Help protect updates from tampering.", "Secblitz puede corregirlo. Ayuda a proteger las actualizaciones contra alteraciones.", "Secblitz peut corriger ce réglage pour mieux protéger les mises à jour des modifications malveillantes.", "Secblitz kann das korrigieren und Updates vor Manipulation schützen.", "O Secblitz pode corrigir isso e ajudar a proteger as atualizações contra adulteração."],
    ["Secblitz can fix this. Stop keeping reusable sign-in secrets after a restart.", "Secblitz puede corregirlo. Tras reiniciar, se dejarán de guardar datos de acceso reutilizables.", "Secblitz peut corriger ce réglage. Après un redémarrage, les secrets de connexion réutilisables ne seront plus conservés.", "Secblitz kann das korrigieren. Nach einem Neustart werden wiederverwendbare Anmeldedaten nicht mehr gespeichert.", "O Secblitz pode corrigir isso. Após reiniciar, os dados de acesso reutilizáveis deixarão de ser guardados."],
    ["Secblitz can fix this. Restore permission prompts after a restart.", "Secblitz puede corregirlo. Recupera las solicitudes de permiso tras reiniciar.", "Secblitz peut corriger ce réglage et rétablir les demandes d’autorisation après un redémarrage.", "Secblitz kann das korrigieren. Nach einem Neustart werden Berechtigungen wieder abgefragt.", "O Secblitz pode corrigir isso e restaurar as solicitações de permissão após reiniciar."],
    ["Secblitz can fix this. Ask for approval before administrator changes.", "Secblitz puede corregirlo. Se pedirá aprobación antes de cambios de administrador.", "Secblitz peut corriger ce réglage pour demander un accord avant les modifications administrateur.", "Secblitz kann das korrigieren und vor Administratoränderungen nach Zustimmung fragen.", "O Secblitz pode corrigir isso e pedir aprovação antes de alterações de administrador."],
    ["Secblitz can fix this. Limit elevated permissions for app installers.", "Secblitz puede corregirlo. Limita los privilegios elevados de los instaladores.", "Secblitz peut corriger ce réglage pour limiter les privilèges élevés des programmes d’installation.", "Secblitz kann das korrigieren und erhöhte Rechte für App-Installer begrenzen.", "O Secblitz pode corrigir isso e limitar privilégios elevados dos instaladores."],
    ["Secblitz can fix this. Limit anonymous access to account names.", "Secblitz puede corregirlo. Limita el acceso anónimo a los nombres de cuenta.", "Secblitz peut corriger ce réglage pour limiter l’accès anonyme aux noms de compte.", "Secblitz kann das korrigieren und anonymen Zugriff auf Kontonamen begrenzen.", "O Secblitz pode corrigir isso e limitar o acesso anônimo aos nomes de conta."],
    ["Secblitz can fix this. Restrict remote sign-ins with blank passwords.", "Secblitz puede corregirlo. Restringe los accesos remotos con contraseñas vacías.", "Secblitz peut corriger ce réglage pour restreindre les connexions à distance avec un mot de passe vide.", "Secblitz kann das korrigieren und Fernanmeldungen mit leeren Passwörtern beschränken.", "O Secblitz pode corrigir isso e restringir acessos remotos com senhas vazias."],
    ["Secblitz can fix this. Turn on this virus protection setting.", "Secblitz puede corregirlo. Activa esta opción de protección antivirus.", "Secblitz peut corriger ce réglage et activer cette protection antivirus.", "Secblitz kann das korrigieren und diese Virenschutzeinstellung aktivieren.", "O Secblitz pode corrigir isso e ativar esta opção de proteção antivírus."],
    ["No action needed for this check.", "No necesitas hacer nada para esta comprobación.", "Aucune action nécessaire pour cette vérification.", "Für diese Prüfung ist nichts zu tun.", "Nenhuma ação necessária para esta verificação."],
    ["This setting was updated and checked.", "Se actualizó y comprobó este ajuste.", "Ce réglage a été mis à jour et vérifié.", "Diese Einstellung wurde aktualisiert und geprüft.", "Esta configuração foi atualizada e verificada."],
    ["Your earlier setting was restored.", "Se restauró tu ajuste anterior.", "Votre ancien réglage a été rétabli.", "Deine frühere Einstellung wurde wiederhergestellt.", "Sua configuração anterior foi restaurada."],
    ["Review saved changes and finish undo before making more changes.", "Revisa los cambios guardados y termina de deshacerlos antes de hacer más cambios.", "Examinez les modifications enregistrées et terminez leur annulation avant d’en faire d’autres.", "Prüfe gespeicherte Änderungen und schließe das Rückgängigmachen ab, bevor du weitere Änderungen vornimmst.", "Revise as alterações salvas e termine de desfazê-las antes de fazer outras mudanças."],
    ["This setting changed since it was saved. Review details before undoing it.", "Este ajuste cambió después de guardarse. Revisa los detalles antes de deshacerlo.", "Ce réglage a changé depuis son enregistrement. Consultez les détails avant de l’annuler.", "Diese Einstellung hat sich seit dem Speichern geändert. Prüfe die Details vor dem Rückgängigmachen.", "Esta configuração mudou desde que foi salva. Veja os detalhes antes de desfazê-la."],
    ["Ask the person or organization managing this PC to review this setting.", "Pide a la persona u organización que administra este PC que revise el ajuste.", "Demandez à la personne ou à l’organisation qui gère ce PC de vérifier ce réglage.", "Bitte die Person oder Organisation, die diesen PC verwaltet, diese Einstellung zu prüfen.", "Peça à pessoa ou organização que gerencia este PC para revisar esta configuração."],
    ["Kept your existing setting. It may already protect you or use Windows defaults; review details if unsure.", "Se conservó tu ajuste. Puede que ya te proteja o use los valores predeterminados de Windows; consulta los detalles si tienes dudas.", "Votre réglage a été conservé. Il vous protège peut-être déjà ou utilise les valeurs par défaut de Windows ; consultez les détails en cas de doute.", "Deine Einstellung wurde beibehalten. Sie schützt dich möglicherweise bereits oder nutzt Windows-Standardwerte; prüfe bei Unsicherheit die Details.", "Sua configuração foi mantida. Ela pode já proteger você ou usar os padrões do Windows; veja os detalhes se tiver dúvidas."],
    ["Save your work and restart your PC to finish this change.", "Guarda tu trabajo y reinicia el PC para completar este cambio.", "Enregistrez votre travail et redémarrez le PC pour terminer cette modification.", "Speichere deine Arbeit und starte den PC neu, um diese Änderung abzuschließen.", "Salve seu trabalho e reinicie o PC para concluir esta alteração."],
    ["Open Windows Security to check which security app is active and healthy.", "Abre Seguridad de Windows para ver qué aplicación de seguridad está activa y funciona correctamente.", "Ouvrez Sécurité Windows pour voir quelle application de sécurité est active et fonctionne correctement.", "Öffne Windows-Sicherheit, um zu prüfen, welche Sicherheits-App aktiv ist und richtig funktioniert.", "Abra Segurança do Windows para verificar qual aplicativo de segurança está ativo e funcionando corretamente."],
    ["Open Windows Security to review virus protection and protection updates.", "Abre Seguridad de Windows para revisar el antivirus y sus actualizaciones.", "Ouvrez Sécurité Windows pour vérifier la protection antivirus et ses mises à jour.", "Öffne Windows-Sicherheit und prüfe Virenschutz und Schutzupdates.", "Abra Segurança do Windows para revisar a proteção antivírus e suas atualizações."],
    ["Check support for your Windows version and edition, including any extended support plan.", "Comprueba el soporte de tu versión y edición de Windows, incluido cualquier plan de soporte ampliado.", "Vérifiez le support de votre version et édition de Windows, y compris tout programme de support étendu.", "Prüfe den Support für deine Windows-Version und -Edition, einschließlich möglicher erweiterter Supportpläne.", "Verifique o suporte da sua versão e edição do Windows, incluindo eventuais planos de suporte estendido."],
    ["Review device encryption and save your recovery key before changing encryption settings.", "Revisa el cifrado del dispositivo y guarda la clave de recuperación antes de cambiar sus ajustes.", "Examinez le chiffrement de l’appareil et sauvegardez votre clé de récupération avant de modifier les réglages.", "Prüfe die Geräteverschlüsselung und sichere deinen Wiederherstellungsschlüssel, bevor du Verschlüsselungseinstellungen änderst.", "Revise a criptografia do dispositivo e salve sua chave de recuperação antes de alterar as configurações."],
    ["Check your PC maker's Secure Boot instructions before changing firmware settings.", "Consulta las instrucciones de Arranque seguro del fabricante antes de cambiar el firmware.", "Consultez les instructions de démarrage sécurisé du fabricant avant de modifier les réglages du micrologiciel.", "Lies die Secure-Boot-Anleitung deines PC-Herstellers, bevor du Firmwareeinstellungen änderst.", "Consulte as instruções de Inicialização Segura do fabricante antes de alterar o firmware."],
    ["Open Windows Update and check for updates. An offline check cannot confirm you are up to date.", "Abre Windows Update y busca actualizaciones. Una comprobación sin conexión no puede confirmar que todo esté al día.", "Ouvrez Windows Update et recherchez des mises à jour. Une vérification hors ligne ne peut pas confirmer que tout est à jour.", "Öffne Windows Update und suche nach Updates. Eine Offline-Prüfung bestätigt nicht, dass alles aktuell ist.", "Abra o Windows Update e procure atualizações. Uma verificação offline não confirma que tudo está atualizado."],
    ["Review Remote Desktop in Settings. Turn it off if you do not use it.", "Revisa Escritorio remoto en Configuración. Desactívalo si no lo usas.", "Vérifiez Bureau à distance dans les Paramètres. Désactivez-le si vous ne l’utilisez pas.", "Prüfe Remotedesktop in den Einstellungen. Schalte ihn aus, wenn du ihn nicht nutzt.", "Revise Área de Trabalho Remota nas Configurações. Desative se não usar."],
    ["Review older device dependencies before turning off SMB1 in Windows Features.", "Comprueba si algún dispositivo antiguo necesita SMB1 antes de desactivarlo en Características de Windows.", "Vérifiez si d’anciens appareils dépendent de SMB1 avant de le désactiver dans les Fonctionnalités Windows.", "Prüfe, ob ältere Geräte SMB1 benötigen, bevor du es in den Windows-Features deaktivierst.", "Verifique se dispositivos antigos dependem de SMB1 antes de desativá-lo nos Recursos do Windows."],
    ["Review reputation-based protection in Windows Security and your browser.", "Revisa la protección basada en reputación en Seguridad de Windows y en tu navegador.", "Vérifiez la protection fondée sur la réputation dans Sécurité Windows et votre navigateur.", "Prüfe den reputationsbasierten Schutz in Windows-Sicherheit und deinem Browser.", "Revise a proteção baseada em reputação na Segurança do Windows e no navegador."],
    ["Review who can sign in. Use unique passwords and extra sign-in verification where supported.", "Revisa quién puede iniciar sesión. Usa contraseñas únicas y verificación adicional cuando esté disponible.", "Vérifiez qui peut se connecter. Utilisez des mots de passe uniques et une vérification supplémentaire si disponible.", "Prüfe, wer sich anmelden kann. Nutze einzigartige Passwörter und zusätzliche Anmeldebestätigung, soweit unterstützt.", "Revise quem pode entrar. Use senhas únicas e verificação adicional de acesso quando disponível."],
    ["Review Core isolation in Windows Security and driver compatibility before enabling memory integrity.", "Revisa Aislamiento del núcleo en Seguridad de Windows y la compatibilidad de los controladores antes de activar la integridad de memoria.", "Vérifiez l’isolation du noyau dans Sécurité Windows et la compatibilité des pilotes avant d’activer l’intégrité de la mémoire.", "Prüfe Kernisolierung in Windows-Sicherheit und die Treiberkompatibilität, bevor du Speicherintegrität aktivierst.", "Revise Isolamento do núcleo na Segurança do Windows e a compatibilidade dos drivers antes de ativar a integridade da memória."],
    ["Review work or school connections in Settings if you are unsure who manages this PC.", "Revisa las conexiones de trabajo o escuela en Configuración si no sabes quién administra este PC.", "Consultez les connexions professionnelles ou scolaires dans les Paramètres si vous ne savez pas qui gère ce PC.", "Prüfe Arbeits- oder Schulkontoverbindungen in den Einstellungen, wenn du nicht weißt, wer diesen PC verwaltet.", "Revise as conexões de trabalho ou escola nas Configurações se não souber quem gerencia este PC."],
    ["Review automatic sign-in and physical access to this PC before changing your sign-in routine.", "Revisa el inicio de sesión automático y el acceso físico al PC antes de cambiar tu forma de entrar.", "Examinez la connexion automatique et l’accès physique à ce PC avant de modifier vos habitudes de connexion.", "Prüfe automatische Anmeldung und physischen Zugriff auf diesen PC, bevor du deine Anmeldung änderst.", "Revise a entrada automática e o acesso físico ao PC antes de mudar sua forma de entrar."],
    ["Review update service permissions with your administrator. Only a separately listed fix can be selected.", "Revisa los permisos de los servicios de actualización con tu administrador. Solo se puede elegir una corrección que aparezca por separado.", "Examinez les autorisations des services de mise à jour avec votre administrateur. Seule une correction proposée séparément peut être sélectionnée.", "Prüfe die Berechtigungen der Updatedienste mit deinem Administrator. Nur separat aufgeführte Korrekturen sind auswählbar.", "Revise as permissões dos serviços de atualização com seu administrador. Só é possível selecionar uma correção listada separadamente."],
    ["Ask your administrator to review antivirus service permissions.", "Pide a tu administrador que revise los permisos del servicio antivirus.", "Demandez à votre administrateur de vérifier les autorisations du service antivirus.", "Bitte deinen Administrator, die Berechtigungen des Antivirusdiensts zu prüfen.", "Peça ao administrador para revisar as permissões do serviço antivírus."],
    ["Ask your administrator to review scheduled task service permissions.", "Pide a tu administrador que revise los permisos del servicio de tareas programadas.", "Demandez à votre administrateur de vérifier les autorisations du service des tâches planifiées.", "Bitte deinen Administrator, die Berechtigungen des Aufgabenplanungsdiensts zu prüfen.", "Peça ao administrador para revisar as permissões do serviço de tarefas agendadas."],
    ["Ask your administrator to review Secblitz monitor service permissions.", "Pide a tu administrador que revise los permisos del servicio de monitorización de Secblitz.", "Demandez à votre administrateur de vérifier les autorisations du service de surveillance Secblitz.", "Bitte deinen Administrator, die Berechtigungen des Secblitz-Monitordiensts zu prüfen.", "Peça ao administrador para revisar as permissões do serviço de monitoramento do Secblitz."],
    ["Review saved changes before undoing them or making more changes.", "Revisa los cambios guardados antes de deshacerlos o hacer más cambios.", "Examinez les modifications enregistrées avant de les annuler ou d’en faire d’autres.", "Prüfe gespeicherte Änderungen, bevor du sie rückgängig machst oder weitere vornimmst.", "Revise as alterações salvas antes de desfazê-las ou fazer outras mudanças."],
    ["Less worry. More protection.", "Menos preocupaciones. Más protección.", "Moins de soucis. Plus de protection.", "Weniger Sorgen. Mehr Schutz.", "Menos preocupação. Mais proteção."],
    ["Your PC, checked.", "Tu PC, revisado.", "Votre PC, vérifié.", "Dein PC, geprüft.", "Seu PC, verificado."],
    ["Protection", "Protección", "Protection", "Schutz", "Proteção"],
    ["Status", "Estado", "État", "Status", "Estado"],
    ["What happens next", "Qué hacer ahora", "La prochaine étape", "So geht es weiter", "O que fazer agora"],
    ["Recommended fixes", "Mejoras recomendadas", "Corrections recommandées", "Empfohlene Korrekturen", "Correções recomendadas"],
    ["Protected", "Protegido", "Protégé", "Geschützt", "Protegido"],
    ["Needs your choice", "Tú decides", "À vous de choisir", "Deine Entscheidung", "Você decide"],
    ["Good to go", "Todo listo", "Tout est prêt", "Alles bereit", "Tudo pronto"],
    ["Can fix", "Se puede corregir", "Correction possible", "Korrektur möglich", "Pode corrigir"],
    ["Fixed", "Corregido", "Corrigé", "Korrigiert", "Corrigido"],
    ["Couldn't check", "No se pudo comprobar", "Vérification impossible", "Prüfung nicht möglich", "Não foi possível verificar"],
    ["Managed elsewhere", "Lo gestiona otra persona", "Géré par un tiers", "Anderweitig verwaltet", "Gerenciado por terceiros"],
    ["Restart needed", "Hay que reiniciar", "Redémarrage nécessaire", "Neustart nötig", "É preciso reiniciar"],
    ["Checking", "Comprobando", "Vérification en cours", "Wird geprüft", "Verificando"],
    ["No checks were returned. Run a new check to review protection.", "No se recibieron resultados. Vuelve a comprobar la protección.", "Aucun résultat reçu. Relancez une vérification de la protection.", "Keine Prüfergebnisse erhalten. Prüfe den Schutz erneut.", "Nenhum resultado recebido. Faça uma nova verificação da proteção."],
    ["Your saved changes were reviewed.", "Se revisaron los cambios guardados.", "Vos modifications enregistrées ont été examinées.", "Deine gespeicherten Änderungen wurden geprüft.", "Suas alterações salvas foram revisadas."],
    ["Your changes are saved. Undo is available for recorded changes.", "Tus cambios están guardados. Puedes deshacer los cambios registrados.", "Vos modifications sont enregistrées. Vous pouvez annuler les changements consignés.", "Deine Änderungen sind gespeichert. Protokollierte Änderungen kannst du rückgängig machen.", "Suas alterações estão salvas. As alterações registradas podem ser desfeitas."],
    ["Live virus protection", "Protección antivirus en tiempo real", "Protection antivirus en temps réel", "Virenabwehr in Echtzeit", "Proteção antivírus em tempo real"],
    ["Suspicious app detection", "Detección de aplicaciones sospechosas", "Détection des applications suspectes", "Verdächtige Apps erkennen", "Detecção de aplicativos suspeitos"],
    ["Downloaded file checks", "Revisión de archivos descargados", "Vérification des fichiers téléchargés", "Heruntergeladene Dateien prüfen", "Verificação de arquivos baixados"],
    ["Compressed file checks", "Revisión de archivos comprimidos", "Vérification des fichiers compressés", "Komprimierte Dateien prüfen", "Verificação de arquivos compactados"],
    ["Work network firewall", "Cortafuegos de la red de trabajo", "Pare-feu du réseau professionnel", "Firewall im Arbeitsnetz", "Firewall da rede de trabalho"],
    ["Home network firewall", "Cortafuegos de la red doméstica", "Pare-feu du réseau domestique", "Firewall im Heimnetz", "Firewall da rede doméstica"],
    ["Public network firewall", "Cortafuegos de la red pública", "Pare-feu du réseau public", "Firewall im öffentlichen Netz", "Firewall da rede pública"],
    ["Work network incoming connections", "Conexiones entrantes en la red de trabajo", "Connexions entrantes du réseau professionnel", "Eingehende Verbindungen im Arbeitsnetz", "Conexões de entrada na rede de trabalho"],
    ["Home network incoming connections", "Conexiones entrantes en la red doméstica", "Connexions entrantes du réseau domestique", "Eingehende Verbindungen im Heimnetz", "Conexões de entrada na rede doméstica"],
    ["Public network incoming connections", "Conexiones entrantes en la red pública", "Connexions entrantes du réseau public", "Eingehende Verbindungen im öffentlichen Netz", "Conexões de entrada na rede pública"],
    ["Permission prompts", "Solicitudes de permiso", "Demandes d’autorisation", "Berechtigungsabfragen", "Solicitações de permissão"],
    ["Administrator approval", "Aprobación del administrador", "Accord de l’administrateur", "Zustimmung des Administrators", "Aprovação do administrador"],
    ["App installation permissions", "Permisos para instalar aplicaciones", "Autorisations d’installation des applications", "Berechtigungen zur App-Installation", "Permissões para instalar aplicativos"],
    ["Account name privacy", "Privacidad de los nombres de cuenta", "Confidentialité des noms de compte", "Kontonamen privat halten", "Privacidade dos nomes de conta"],
    ["Remote sign-in safeguards", "Protección del inicio de sesión remoto", "Protection des connexions à distance", "Schutz bei Fernanmeldungen", "Proteção do acesso remoto"],
    ["Sign-in secret protection", "Protección de los datos de acceso", "Protection des secrets de connexion", "Anmeldedaten schützen", "Proteção dos dados de acesso"],
    ["Update download protection", "Protección de la descarga de actualizaciones", "Protection du téléchargement des mises à jour", "Update-Downloads schützen", "Proteção do download de atualizações"],
    ["Windows Update tamper protection", "Protección de Windows Update contra alteraciones", "Protection de Windows Update contre les modifications malveillantes", "Windows Update vor Manipulation schützen", "Proteção do Windows Update contra adulteração"],
    ["Additional protection checks", "Más comprobaciones de protección", "Vérifications de protection supplémentaires", "Weitere Schutzprüfungen", "Mais verificações de proteção"],
    ["Protection check", "Comprobación de protección", "Vérification de la protection", "Schutzprüfung", "Verificação da proteção"],
    ["Your security apps", "Tus aplicaciones de seguridad", "Vos applications de sécurité", "Deine Sicherheits-Apps", "Seus aplicativos de segurança"],
    ["Network protection", "Protección de la red", "Protection du réseau", "Netzwerkschutz", "Proteção da rede"],
    ["Virus protection", "Protección antivirus", "Protection antivirus", "Virenschutz", "Proteção antivírus"],
    ["Windows support", "Soporte de Windows", "Support de Windows", "Windows-Support", "Suporte do Windows"],
    ["Protection if your PC is lost", "Protección si pierdes tu PC", "Protection en cas de perte du PC", "Schutz bei Verlust deines PCs", "Proteção se você perder o PC"],
    ["Startup protection", "Protección al arrancar", "Protection au démarrage", "Schutz beim Start", "Proteção na inicialização"],
    ["Remote access", "Acceso remoto", "Accès à distance", "Fernzugriff", "Acesso remoto"],
    ["Older file sharing", "Uso compartido de archivos antiguo", "Ancien partage de fichiers", "Ältere Dateifreigabe", "Compartilhamento antigo de arquivos"],
    ["Unsafe app and website warnings", "Avisos de aplicaciones y sitios peligrosos", "Alertes sur les applications et sites dangereux", "Warnungen vor unsicheren Apps und Websites", "Avisos de aplicativos e sites perigosos"],
    ["Account sign-in safety", "Seguridad del acceso a las cuentas", "Sécurité de connexion aux comptes", "Sichere Kontoanmeldung", "Segurança do acesso às contas"],
    ["Core system protection", "Protección del núcleo del sistema", "Protection du cœur du système", "Schutz des Systemkerns", "Proteção do núcleo do sistema"],
    ["Who manages this PC", "Quién administra este PC", "Qui gère ce PC", "Wer diesen PC verwaltet", "Quem gerencia este PC"],
    ["Automatic sign-in", "Inicio de sesión automático", "Connexion automatique", "Automatische Anmeldung", "Entrada automática"],
    ["Update download permissions", "Permisos de descarga de actualizaciones", "Autorisations de téléchargement des mises à jour", "Berechtigungen für Update-Downloads", "Permissões de download de atualizações"],
    ["Windows Update permissions", "Permisos de Windows Update", "Autorisations de Windows Update", "Windows-Update-Berechtigungen", "Permissões do Windows Update"],
    ["Antivirus service permissions", "Permisos del servicio antivirus", "Autorisations du service antivirus", "Berechtigungen des Antivirusdiensts", "Permissões do serviço antivírus"],
    ["Scheduled task service permissions", "Permisos del servicio de tareas programadas", "Autorisations du service des tâches planifiées", "Berechtigungen des Aufgabenplanungsdiensts", "Permissões do serviço de tarefas agendadas"],
    ["Protection monitor permissions", "Permisos del monitor de protección", "Autorisations du moniteur de protection", "Berechtigungen des Schutzmonitors", "Permissões do monitor de proteção"],
    ["Saved changes", "Cambios guardados", "Modifications enregistrées", "Gespeicherte Änderungen", "Alterações salvas"],
    ["readback differs from recorded target; pending transaction", "la relectura difiere del objetivo registrado; transacción pendiente", "la relecture diffère de la cible enregistrée ; transaction en attente", "Rücklesewert weicht vom protokollierten Ziel ab; ausstehende Transaktion", "a releitura difere do destino registrado; transação pendente"],
    ["readback differs from original; pending transaction", "la relectura difiere del original; transacción pendiente", "la relecture diffère de l’original ; transaction en attente", "Rücklesewert weicht vom Original ab; ausstehende Transaktion", "a releitura difere do original; transação pendente"],
    ["Unsupported service access mask requires manual review", "Una máscara de acceso de servicio no compatible requiere revisión manual", "Un masque d’accès au service non pris en charge exige une vérification manuelle", "Nicht unterstützte Dienstzugriffsmaske erfordert manuelle Prüfung", "Máscara de acesso ao serviço não suportada exige revisão manual"],
    ["DACL offset without DACL_PRESENT", "Desplazamiento de DACL sin DACL_PRESENT", "Décalage de DACL sans DACL_PRESENT", "DACL-Versatz ohne DACL_PRESENT", "Deslocamento de DACL sem DACL_PRESENT"],
    ["Deny, inherited or unsupported descriptor, ACE or access-mask semantics require manual evaluation; no dangerous supported ALLOW candidate found. No automatic repair.", "La semántica de descriptores, ACE o máscaras de acceso con denegación, herencia o no compatibles requiere evaluación manual; no se encontró ninguna ACE ALLOW peligrosa de tipo compatible. Sin corrección automática.", "La sémantique des descripteurs, ACE ou masques d’accès avec refus, héritage ou non pris en charge exige une évaluation manuelle ; aucune ACE ALLOW dangereuse d’un type pris en charge n’a été trouvée. Aucune correction automatique.", "Verweigernde, geerbte oder nicht unterstützte Deskriptor-, ACE- oder Zugriffsmaskensemantik erfordert manuelle Bewertung; keine gefährliche unterstützte ALLOW-ACE gefunden. Keine automatische Reparatur.", "A semântica de descritores, ACEs ou máscaras de acesso com negação, herança ou não suportados exige avaliação manual; nenhuma ACE ALLOW perigosa de tipo suportado foi encontrada. Sem correção automática."],
    ["Invalid SID header", "Cabecera de SID no válida", "En-tête du SID non valide", "Ungültiger SID-Kopf", "Cabeçalho de SID inválido"],
    ["Invalid SID length", "Longitud de SID no válida", "Longueur du SID non valide", "Ungültige SID-Länge", "Comprimento de SID inválido"],
    ["Invalid security descriptor header", "Cabecera del descriptor de seguridad no válida", "En-tête du descripteur de sécurité non valide", "Ungültiger Sicherheitsdeskriptorkopf", "Cabeçalho do descritor de segurança inválido"],
    ["Descriptor is not self-relative", "El descriptor no es autorrelativo", "Le descripteur n’est pas auto-relatif", "Deskriptor ist nicht selbstrelativ", "O descritor não é autorrelativo"],
    ["DACL outside descriptor", "DACL fuera del descriptor", "DACL hors du descripteur", "DACL außerhalb des Deskriptors", "DACL fora do descritor"],
    ["Truncated ACL", "ACL truncada", "ACL tronquée", "Abgeschnittene ACL", "ACL truncada"],
    ["Truncated ACE header", "Cabecera de ACE truncada", "En-tête ACE tronqué", "Abgeschnittener ACE-Kopf", "Cabeçalho ACE truncado"],
    ["Missing ACE SID", "Falta el SID de la ACE", "SID de l’ACE manquant", "ACE-SID fehlt", "SID da ACE ausente"],
    ["ALLOW mask", "máscara ALLOW", "masque ALLOW", "ALLOW-Maske", "máscara ALLOW"],
    ["risky bits", "bits de riesgo", "bits à risque", "riskante Bits", "bits de risco"],
    ["Invalid service descriptor size", "Tamaño del descriptor del servicio no válido", "Taille du descripteur du service non valide", "Ungültige Dienstdeskriptorgröße", "Tamanho do descritor do serviço inválido"],
    ["Config string outside buffer", "Cadena de configuración fuera del búfer", "Chaîne de configuration hors du tampon", "Konfigurationszeichenfolge außerhalb des Puffers", "Cadeia de configuração fora do buffer"],
    ["Invalid config string offset", "Desplazamiento de cadena de configuración no válido", "Décalage de chaîne de configuration non valide", "Ungültiger Versatz der Konfigurationszeichenfolge", "Deslocamento da cadeia de configuração inválido"],
    ["Unterminated service config string", "Cadena de configuración del servicio sin terminador", "Chaîne de configuration du service non terminée", "Dienstkonfigurationszeichenfolge ohne Abschlusszeichen", "Cadeia de configuração do serviço sem terminador"],
    ["Invalid service config size", "Tamaño de configuración del servicio no válido", "Taille de configuration du service non valide", "Ungültige Dienstkonfigurationsgröße", "Tamanho da configuração do serviço inválido"],
    ["Unexpected built-in service type", "Tipo de servicio integrado inesperado", "Type de service intégré inattendu", "Unerwarteter Typ des integrierten Diensts", "Tipo de serviço integrado inesperado"],
    ["Unexpected built-in service account", "Cuenta de servicio integrado inesperada", "Compte du service intégré inattendu", "Unerwartetes Konto des integrierten Diensts", "Conta do serviço integrado inesperada"],
    ["Cannot resolve Windows system directory", "No se puede resolver el directorio de sistema de Windows", "Impossible de résoudre le dossier système Windows", "Windows-Systemverzeichnis kann nicht aufgelöst werden", "Não foi possível resolver o diretório de sistema do Windows"],
    ["Invalid system directory", "Directorio de sistema no válido", "Dossier système non valide", "Ungültiges Systemverzeichnis", "Diretório de sistema inválido"],
    ["Unexpected built-in service executable configuration", "Configuración inesperada del ejecutable del servicio integrado", "Configuration inattendue de l’exécutable du service intégré", "Unerwartete Programmkonfiguration des integrierten Diensts", "Configuração inesperada do executável do serviço integrado"],
    ["Service host is not a regular non-reparse file", "El host del servicio no es un archivo normal libre de puntos de reanálisis", "L’hôte du service n’est pas un fichier ordinaire sans point de réanalyse", "Diensthost ist keine reguläre Datei ohne Analysepunkt", "O host do serviço não é um arquivo regular sem ponto de nova análise"],
    ["Invalid service host owner", "Propietario del host del servicio no válido", "Propriétaire de l’hôte du service non valide", "Ungültiger Dienstehost-Eigentümer", "Proprietário do host do serviço inválido"],
    ["Invalid service host SID size", "Tamaño del SID del host del servicio no válido", "Taille du SID de l’hôte du service non valide", "Ungültige Dienstehost-SID-Größe", "Tamanho do SID do host do serviço inválido"],
    ["Eligible service permission repair", "Corrección de permisos del servicio permitida", "Correction des autorisations du service admissible", "Dienstberechtigungskorrektur zulässig", "Correção de permissões do serviço permitida"],
    ["Service permissions preserved: ", "Permisos del servicio conservados: ", "Autorisations du service conservées : ", "Dienstberechtigungen beibehalten: ", "Permissões do serviço preservadas: "],
    ["Service descriptor changed before write", "El descriptor del servicio cambió antes de escribir", "Le descripteur du service a changé avant l’écriture", "Dienstdeskriptor vor dem Schreiben geändert", "O descritor do serviço mudou antes da gravação"],
    ["Service DACL exact readback mismatch; mutation outcome requires review", "La relectura exacta de la DACL del servicio no coincide; revise el resultado del cambio", "La relecture exacte de la DACL du service ne correspond pas ; vérifiez le résultat de la modification", "Exakt zurückgelesene Dienst-DACL stimmt nicht überein; Änderungsergebnis prüfen", "A releitura exata da DACL do serviço não corresponde; revise o resultado da alteração"],
    ["Service permissions: ", "Permisos del servicio: ", "Autorisations du service : ", "Dienstberechtigungen: ", "Permissões do serviço: "],
    ["Service is not installed; no DACL assessed.", "El servicio no está instalado; no se evaluó ninguna DACL.", "Le service n’est pas installé ; aucune DACL évaluée.", "Dienst ist nicht installiert; keine DACL bewertet.", "O serviço não está instalado; nenhuma DACL avaliada."],
    ["Invalid security descriptor length", "Longitud del descriptor de seguridad no válida", "Longueur du descripteur de sécurité non valide", "Ungültige Sicherheitsdeskriptorlänge", "Comprimento do descritor de segurança inválido"],
    ["review", "revisar", "à examiner", "prüfen", "revisar"],
    ["Absent or NULL DACL permits unrestricted access. Administrator investigation required; no automatic repair.", "Una DACL ausente o NULL permite acceso sin restricciones. Se requiere investigación del administrador; no hay corrección automática.", "Une DACL absente ou NULL permet un accès sans restriction. Une investigation par l’administrateur est nécessaire ; aucune correction automatique.", "Fehlende oder NULL-DACL erlaubt uneingeschränkten Zugriff. Untersuchung durch Administrator erforderlich; keine automatische Reparatur.", "Uma DACL ausente ou NULL permite acesso irrestrito. É necessária investigação pelo administrador; sem correção automática."],
    ["Candidate dangerous broad-principal grants: ", "Posibles concesiones peligrosas a grupos amplios: ", "Droits potentiellement dangereux accordés à des groupes étendus : ", "Möglicherweise gefährliche Berechtigungen für weit gefasste Gruppen: ", "Possíveis concessões perigosas a grupos amplos: "],
    ["Deny, inherited or unsupported ACE semantics require manual evaluation. ", "La semántica de ACE de denegación, heredadas o no compatibles requiere evaluación manual. ", "La sémantique des ACE de refus, héritées ou non prises en charge exige une évaluation manuelle. ", "Semantik verweigernder, geerbter oder nicht unterstützter ACEs erfordert manuelle Bewertung. ", "A semântica de ACEs de negação, herdadas ou não suportadas exige avaliação manual. "],
    ["This is an ACE scan, not effective access or proof of exploitability. ", "Es un análisis de ACE, no una evaluación del acceso efectivo ni una prueba de explotabilidad. ", "Il s’agit d’une analyse des ACE, pas d’une évaluation de l’accès effectif ni d’une preuve d’exploitabilité. ", "Dies ist eine ACE-Prüfung, keine Bewertung wirksamer Zugriffsrechte oder ein Nachweis der Ausnutzbarkeit. ", "Esta é uma análise de ACEs, não uma avaliação do acesso efetivo nem uma prova de exploração possível. "],
    ["Consult the fixed service repair control for gated eligibility.", "Consulte el control de corrección específico del servicio para comprobar sus requisitos.", "Consultez le contrôle de correction prédéfini du service pour vérifier son admissibilité.", "Voraussetzungen im festen Reparaturkontrollpunkt des Diensts prüfen.", "Consulte o controle de correção específico do serviço para verificar seus requisitos."],
    ["Review with the service owner; no automatic repair for this service.", "Revise con el responsable del servicio; no hay corrección automática para este servicio.", "Examinez avec le responsable du service ; aucune correction automatique pour ce service.", "Mit dem Dienstverantwortlichen prüfen; keine automatische Reparatur für diesen Dienst.", "Revise com o responsável pelo serviço; sem correção automática para este serviço."],
    ["Deny, inherited or unsupported ACE semantics require manual evaluation; no dangerous supported ALLOW candidate found. No automatic repair.", "La semántica de ACE de denegación, heredadas o no compatibles requiere evaluación manual; no se encontró ninguna ACE ALLOW peligrosa de tipo compatible. Sin corrección automática.", "La sémantique des ACE de refus, héritées ou non prises en charge exige une évaluation manuelle ; aucune ACE ALLOW dangereuse d’un type pris en charge n’a été trouvée. Aucune correction automatique.", "Semantik verweigernder, geerbter oder nicht unterstützter ACEs erfordert manuelle Bewertung; keine gefährliche unterstützte ALLOW-ACE gefunden. Keine automatische Reparatur.", "A semântica de ACEs de negação, herdadas ou não suportadas exige avaliação manual; nenhuma ACE ALLOW perigosa de tipo suportado foi encontrada. Sem correção automática."],
    ["No dangerous ALLOW bits found for Everyone, Authenticated Users or Builtin Users in this DACL. Limited scan: other principals, ownership and executable paths were not assessed.", "No se encontraron bits ALLOW peligrosos para Everyone, Authenticated Users o Builtin Users en esta DACL. Análisis limitado: no se evaluaron otras entidades, la propiedad ni las rutas de ejecutables.", "Aucun bit ALLOW dangereux trouvé pour Everyone, Authenticated Users ou Builtin Users dans cette DACL. Analyse limitée : les autres identités, la propriété et les chemins des exécutables n’ont pas été évalués.", "Keine gefährlichen ALLOW-Bits für Everyone, Authenticated Users oder Builtin Users in dieser DACL gefunden. Begrenzte Prüfung: andere Berechtigte, Eigentümer und Programmpfade wurden nicht bewertet.", "Nenhum bit ALLOW perigoso encontrado para Everyone, Authenticated Users ou Builtin Users nesta DACL. Análise limitada: outras entidades, propriedade e caminhos de executáveis não foram avaliados."],
    ["Cannot connect to local SCM: ", "No se puede conectar al SCM local: ", "Impossible de se connecter au SCM local : ", "Verbindung mit lokalem SCM nicht möglich: ", "Não foi possível conectar ao SCM local: "],
    ["DACL could not be assessed: ", "No se pudo evaluar la DACL: ", "Impossible d’évaluer la DACL : ", "DACL konnte nicht bewertet werden: ", "Não foi possível avaliar a DACL: "],
    ["No change made.", "No se realizaron cambios.", "Aucune modification effectuée.", "Keine Änderung vorgenommen.", "Nenhuma alteração realizada."],
    ["Service permission eligibility requires Windows", "La elegibilidad para modificar permisos de servicios requiere Windows", "L’admissibilité aux modifications des autorisations de service exige Windows", "Änderungsvoraussetzungen für Dienstberechtigungen erfordern Windows", "A elegibilidade para alterar permissões de serviços exige Windows"],
    ["Unknown service permission control id", "Identificador de control de permisos de servicio desconocido", "Identifiant de contrôle des autorisations de service inconnu", "Unbekannte Kontrollkennung für Dienstberechtigungen", "Identificador de controle de permissões de serviço desconhecido"],
    ["Invalid platform action arguments", "Argumentos de acción de plataforma no válidos", "Arguments d’action de plateforme non valides", "Ungültige Argumente für Plattformaktion", "Argumentos de ação da plataforma inválidos"],
    ["Service permission gate was not acknowledged", "No se confirmó la comprobación de requisitos para los permisos del servicio", "La vérification des conditions d’autorisation du service n’a pas été confirmée", "Voraussetzungsprüfung für Dienstberechtigungen wurde nicht bestätigt", "A verificação dos requisitos de permissões do serviço não foi confirmada"],
    ["Computer Group Policy evidence: service permissions are assessment only", "Indicios de directiva de grupo del equipo: los permisos de servicios solo se evalúan", "Indices de stratégie de groupe ordinateur : évaluation uniquement des autorisations de service", "Hinweise auf Computergruppenrichtlinien: Dienstberechtigungen werden nur bewertet", "Indícios de política de grupo do computador: permissões de serviços apenas para avaliação"],
    ["Applied computer policy settings: service permissions are assessment only", "Ajustes de directiva del equipo aplicados: los permisos de servicios solo se evalúan", "Paramètres de stratégie ordinateur appliqués : évaluation uniquement des autorisations de service", "Angewendete Computerrichtlinien: Dienstberechtigungen werden nur bewertet", "Configurações de política do computador aplicadas: permissões de serviços apenas para avaliação"],
    ["Local computer service policy artifacts: assessment only", "Indicios de directiva local de servicios del equipo: solo evaluación", "Traces de stratégie locale des services de l’ordinateur : évaluation uniquement", "Lokale Computerdienstrichtlinien vorhanden: nur Bewertung", "Indícios de política local de serviços do computador: apenas avaliação"],
    ["Invalid service permission gate request", "Solicitud de comprobación de permisos de servicio no válida", "Demande de vérification des autorisations de service non valide", "Ungültige Anfrage zur Voraussetzungsprüfung für Dienstberechtigungen", "Solicitação de verificação de permissões de serviço inválida"],
    ["Service permissions require the native wrapper", "Los permisos de servicios requieren el adaptador nativo", "Les autorisations de service exigent l’adaptateur natif", "Dienstberechtigungen erfordern den nativen Adapter", "Permissões de serviços exigem o adaptador nativo"],
    ["Repair dangerous BITS service permissions", "Corregir permisos peligrosos del servicio BITS", "Corriger les autorisations dangereuses du service BITS", "Gefährliche Berechtigungen des BITS-Diensts korrigieren", "Corrigir permissões perigosas do serviço BITS"],
    ["Repair dangerous Windows Update service permissions", "Corregir permisos peligrosos del servicio Windows Update", "Corriger les autorisations dangereuses du service Windows Update", "Gefährliche Berechtigungen des Windows-Update-Diensts korrigieren", "Corrigir permissões perigosas do serviço Windows Update"],
    ["Repair dangerous ", "Corregir permisos peligrosos de ", "Corriger les autorisations dangereuses de ", "Gefährliche Berechtigungen korrigieren: ", "Corrigir permissões perigosas de "],
    [" service permissions", " (servicio)", " (service)", " (Dienst)", " (serviço)"],
    ["Remove dangerous explicit broad-principal service grants; preserve other ACE bytes and require exact-state rollback.", "Eliminar concesiones explícitas peligrosas a grupos amplios en el servicio; conservar los demás bytes ACE y exigir restauración del estado exacto.", "Supprimer les droits explicites dangereux accordés à des groupes étendus sur le service ; conserver les autres octets ACE et exiger un retour à l’état exact.", "Gefährliche ausdrückliche Dienstberechtigungen für weit gefasste Gruppen entfernen; andere ACE-Bytes beibehalten und exakte Zustandswiederherstellung verlangen.", "Remover concessões explícitas perigosas a grupos amplos no serviço; preservar os demais bytes ACE e exigir restauração do estado exato."],
    ["Unknown service permission control", "Control de permisos de servicio desconocido", "Contrôle des autorisations de service inconnu", "Unbekannte Kontrolle für Dienstberechtigungen", "Controle de permissões de serviço desconhecido"],
    ["Service permission observation requires Windows", "La lectura de permisos de servicios requiere Windows", "La lecture des autorisations de service exige Windows", "Lesen von Dienstberechtigungen erfordert Windows", "A leitura de permissões de serviços exige Windows"],
    ["Service permission repair requires Windows", "La corrección de permisos de servicios requiere Windows", "La correction des autorisations de service exige Windows", "Korrektur von Dienstberechtigungen erfordert Windows", "A correção de permissões de serviços exige Windows"],
    ["Service permission auditing requires Windows", "La auditoría de permisos de servicios requiere Windows", "L’audit des autorisations de service exige Windows", "Prüfung von Dienstberechtigungen erfordert Windows", "A auditoria de permissões de serviços exige Windows"],
    ["Service permission audit", "Auditoría de permisos de servicios", "Audit des autorisations de service", "Prüfung der Dienstberechtigungen", "Auditoria de permissões de serviços"],
    ["Truncated WORD", "WORD truncado", "WORD tronqué", "Abgeschnittenes WORD", "WORD truncado"],
    ["Truncated DWORD", "DWORD truncado", "DWORD tronqué", "Abgeschnittenes DWORD", "DWORD truncado"],
    ["Invalid SID", "SID no válido", "SID non valide", "Ungültige SID", "SID inválido"],
    ["Invalid SID size", "Tamaño de SID no válido", "Taille du SID non valide", "Ungültige SID-Größe", "Tamanho de SID inválido"],
    ["Service owner is not SYSTEM, Administrators or TrustedInstaller", "El propietario del servicio no es SYSTEM, Administrators ni TrustedInstaller", "Le propriétaire du service n’est ni SYSTEM, ni Administrators, ni TrustedInstaller", "Diensteigentümer ist weder SYSTEM noch Administrators noch TrustedInstaller", "O proprietário do serviço não é SYSTEM, Administrators nem TrustedInstaller"],
    ["Service owner, group or descriptor flags changed", "Cambió el propietario, grupo o indicadores del descriptor del servicio", "Le propriétaire, le groupe ou les indicateurs du descripteur du service ont changé", "Diensteigentümer, Gruppe oder Deskriptorflags geändert", "O proprietário, grupo ou indicadores do descritor do serviço mudaram"],
    ["Service DACL drift or non-repair transition", "Desviación de DACL del servicio o transición ajena a la corrección", "Écart de DACL du service ou transition étrangère à la correction", "Dienst-DACL abgewichen oder Übergang entspricht keiner Reparatur", "Desvio da DACL do serviço ou transição alheia à correção"],
    ["Invalid descriptor header", "Cabecera de descriptor no válida", "En-tête du descripteur non valide", "Ungültiger Deskriptorkopf", "Cabeçalho do descritor inválido"],
    ["Unsupported descriptor flags", "Indicadores de descriptor no compatibles", "Indicateurs de descripteur non pris en charge", "Nicht unterstützte Deskriptorflags", "Indicadores do descritor não suportados"],
    ["SACL snapshots are forbidden", "Las instantáneas SACL están prohibidas", "Les instantanés SACL sont interdits", "SACL-Momentaufnahmen sind unzulässig", "Instantâneos SACL são proibidos"],
    ["Invalid SID offset", "Desplazamiento de SID no válido", "Décalage du SID non valide", "Ungültiger SID-Versatz", "Deslocamento de SID inválido"],
    ["Truncated SID", "SID truncado", "SID tronqué", "Abgeschnittene SID", "SID truncado"],
    ["Invalid DACL offset", "Desplazamiento de DACL no válido", "Décalage de DACL non valide", "Ungültiger DACL-Versatz", "Deslocamento de DACL inválido"],
    ["Truncated DACL", "DACL truncada", "DACL tronquée", "Abgeschnittene DACL", "DACL truncada"],
    ["Overlapping descriptor sections", "Secciones del descriptor superpuestas", "Sections du descripteur qui se chevauchent", "Überlappende Deskriptorabschnitte", "Seções do descritor sobrepostas"],
    ["Service DACL state must be a string", "El estado DACL del servicio debe ser una cadena", "L’état de DACL du service doit être une chaîne", "Dienst-DACL-Zustand muss eine Zeichenfolge sein", "O estado DACL do serviço deve ser uma cadeia de texto"],
    ["Service DACL state exceeds limit", "El estado DACL del servicio supera el límite", "L’état de DACL du service dépasse la limite", "Dienst-DACL-Zustand überschreitet das Limit", "O estado DACL do serviço excede o limite"],
    ["Invalid service DACL state version", "Versión de estado DACL del servicio no válida", "Version de l’état de DACL du service non valide", "Ungültige Version des Dienst-DACL-Zustands", "Versão do estado DACL do serviço inválida"],
    ["Noncanonical DACL hex", "DACL hexadecimal no canónica", "DACL hexadécimale non canonique", "Nichtkanonische DACL-Hexadezimaldarstellung", "DACL hexadecimal não canônica"],
    ["Noncanonical DACL descriptor offsets or trailing bytes", "Desplazamientos del descriptor DACL no canónicos o bytes sobrantes", "Décalages non canoniques du descripteur DACL ou octets supplémentaires", "Nichtkanonische DACL-Deskriptorversätze oder nachgestellte Bytes", "Deslocamentos não canônicos do descritor DACL ou bytes excedentes"],
    ["NULL or absent DACL requires manual review", "Una DACL NULL o ausente requiere revisión manual", "Une DACL NULL ou absente exige une vérification manuelle", "NULL-DACL oder fehlende DACL erfordert manuelle Prüfung", "DACL NULL ou ausente exige revisão manual"],
    ["Deny, inherited, flagged or unsupported ACE requires manual review", "Una ACE de denegación, heredada, con indicadores o no compatible requiere revisión manual", "Une ACE de refus, héritée, avec indicateurs ou non prise en charge exige une vérification manuelle", "Verweigernde, geerbte, markierte oder nicht unterstützte ACE erfordert manuelle Prüfung", "ACE de negação, herdada, com indicadores ou não suportada exige revisão manual"],
    ["Invalid ACL header", "Cabecera de ACL no válida", "En-tête ACL non valide", "Ungültiger ACL-Kopf", "Cabeçalho ACL inválido"],
    ["Invalid ACL size", "Tamaño de ACL no válido", "Taille ACL non valide", "Ungültige ACL-Größe", "Tamanho ACL inválido"],
    ["Truncated ACE", "ACE truncada", "ACE tronquée", "Abgeschnittene ACE", "ACE truncada"],
    ["Invalid ACE size", "Tamaño de ACE no válido", "Taille ACE non valide", "Ungültige ACE-Größe", "Tamanho ACE inválido"],
    ["Truncated ALLOW/DENY ACE", "ACE ALLOW/DENY truncada", "ACE ALLOW/DENY tronquée", "Abgeschnittene ALLOW/DENY-ACE", "ACE ALLOW/DENY truncada"],
    ["Disable always-elevated MSI installation", "Desactivar la instalación MSI siempre elevada", "Désactiver l’installation MSI toujours élevée", "MSI-Installation mit stets erhöhten Rechten deaktivieren", "Desativar a instalação MSI sempre elevada"],
    ["Restrict anonymous SAM enumeration", "Restringir la enumeración anónima de SAM", "Restreindre l’énumération anonyme SAM", "Anonyme SAM-Auflistung einschränken", "Restringir a enumeração anônima do SAM"],
    ["Limit blank-password accounts to console logon", "Limitar las cuentas sin contraseña al inicio de sesión en consola", "Limiter les comptes sans mot de passe à la connexion sur console", "Konten mit leerem Passwort auf Konsolenanmeldung beschränken", "Limitar contas sem senha ao logon no console"],
    ["Disable WDigest plaintext credential caching", "Desactivar la caché de credenciales en texto claro de WDigest", "Désactiver le stockage en cache des identifiants WDigest en clair", "Zwischenspeicherung von WDigest-Anmeldedaten im Klartext deaktivieren", "Desativar o cache de credenciais WDigest em texto simples"],
    ["Repair only machine AlwaysInstallElevated=1. The machine setting breaks the vulnerable machine/user conjunction; preserve HKCU, absent values and normal administrator-authorized installs.", "Corregir solo AlwaysInstallElevated=1 del equipo. El ajuste del equipo rompe la combinación vulnerable de equipo y usuario; conservar HKCU, valores ausentes e instalaciones normales autorizadas por el administrador.", "Corriger uniquement AlwaysInstallElevated=1 au niveau machine. Ce réglage rompt la combinaison vulnérable machine/utilisateur ; conserver HKCU, les valeurs absentes et les installations normales autorisées par l’administrateur.", "Nur AlwaysInstallElevated=1 auf Computerebene korrigieren. Die Computereinstellung unterbricht die verwundbare Kombination aus Computer und Benutzer; HKCU, fehlende Werte und regulär vom Administrator genehmigte Installationen beibehalten.", "Corrigir apenas AlwaysInstallElevated=1 da máquina. A configuração da máquina rompe a combinação vulnerável de máquina e usuário; preservar HKCU, valores ausentes e instalações normais autorizadas pelo administrador."],
    ["Repair only RestrictAnonymousSAM=0. Require authentication for account enumeration; legacy anonymous enumeration workflows may be affected. Preserve absent values and other LSA settings.", "Corregir solo RestrictAnonymousSAM=0. Exigir autenticación para enumerar cuentas; puede afectar a procesos antiguos de enumeración anónima. Conservar valores ausentes y otros ajustes LSA.", "Corriger uniquement RestrictAnonymousSAM=0. Exiger l’authentification pour énumérer les comptes ; les anciens usages d’énumération anonyme peuvent être affectés. Conserver les valeurs absentes et les autres réglages LSA.", "Nur RestrictAnonymousSAM=0 korrigieren. Authentifizierung zur Kontoauflistung verlangen; ältere Abläufe mit anonymer Auflistung können beeinträchtigt werden. Fehlende Werte und andere LSA-Einstellungen beibehalten.", "Corrigir apenas RestrictAnonymousSAM=0. Exigir autenticação para enumerar contas; fluxos antigos de enumeração anônima podem ser afetados. Preservar valores ausentes e outras configurações LSA."],
    ["Repair only LimitBlankPasswordUse=0. Block remote logons using blank local passwords while preserving physical console logon. Preserve absent values; no passwords are inspected or changed.", "Corregir solo LimitBlankPasswordUse=0. Bloquear inicios de sesión remotos con contraseñas locales vacías y conservar el acceso desde la consola física. Conservar valores ausentes; no se inspeccionan ni cambian contraseñas.", "Corriger uniquement LimitBlankPasswordUse=0. Bloquer les connexions distantes avec mot de passe local vide tout en conservant la connexion sur console physique. Conserver les valeurs absentes ; aucun mot de passe n’est inspecté ni modifié.", "Nur LimitBlankPasswordUse=0 korrigieren. Fernanmeldungen mit leeren lokalen Passwörtern blockieren, Anmeldung an der physischen Konsole beibehalten. Fehlende Werte erhalten; keine Passwörter werden geprüft oder geändert.", "Corrigir apenas LimitBlankPasswordUse=0. Bloquear logons remotos com senhas locais vazias, preservando o logon no console físico. Preservar valores ausentes; nenhuma senha é inspecionada ou alterada."],
    ["Repair only UseLogonCredential=1. Preserve absent values (safe on supported Windows). Readback verifies stored configuration, not running LSASS; restart/sign-out may be needed for existing sessions. Legacy Digest SSO may require credentials.", "Corregir solo UseLogonCredential=1. Conservar valores ausentes (seguros en Windows compatible). La relectura verifica la configuración guardada, no LSASS en ejecución; las sesiones existentes pueden requerir reinicio o cierre de sesión. El SSO Digest antiguo puede pedir credenciales.", "Corriger uniquement UseLogonCredential=1. Conserver les valeurs absentes (sûres sous Windows pris en charge). La relecture vérifie la configuration enregistrée, pas LSASS en cours ; un redémarrage ou une déconnexion peut être nécessaire pour les sessions existantes. L’ancien SSO Digest peut demander des identifiants.", "Nur UseLogonCredential=1 korrigieren. Fehlende Werte beibehalten (unter unterstütztem Windows sicher). Erneutes Lesen prüft die gespeicherte Konfiguration, nicht den laufenden LSASS; bestehende Sitzungen benötigen eventuell Neustart oder Abmeldung. Älteres Digest-SSO kann Anmeldedaten anfordern.", "Corrigir apenas UseLogonCredential=1. Preservar valores ausentes (seguros no Windows compatível). A releitura verifica a configuração salva, não o LSASS em execução; sessões existentes podem exigir reinicialização ou saída. O SSO Digest antigo pode solicitar credenciais."],
    ["Invalid registry DWORD", "DWORD del registro no válido", "DWORD du registre non valide", "Ungültiger Registrierungs-DWORD-Wert", "DWORD do registro inválido"],
    ["Other machine Installer policy is configured: assessment only", "Hay otra directiva de Installer configurada en el equipo: solo evaluación", "Une autre stratégie Installer est configurée au niveau machine : évaluation uniquement", "Weitere Installer-Computerrichtlinie konfiguriert: nur Bewertung", "Outra política do Installer está configurada na máquina: apenas avaliação"],
    ["Invalid binary registry state", "Estado binario del registro no válido", "État binaire du registre non valide", "Ungültiger binärer Registrierungszustand", "Estado binário do registro inválido"],
    ["Invalid binary registry DWORD", "DWORD binario del registro no válido", "DWORD binaire du registre non valide", "Ungültiger binärer Registrierungs-DWORD-Wert", "DWORD binário do registro inválido"],
    ["Unknown privilege control", "Control de privilegios desconocido", "Contrôle des privilèges inconnu", "Unbekannte Berechtigungskontrolle", "Controle de privilégios desconhecido"],
    ["Privilege preference is not a DWORD", "La preferencia de privilegios no es DWORD", "Le réglage des privilèges n’est pas un DWORD", "Berechtigungseinstellung ist kein DWORD", "A preferência de privilégios não é DWORD"],
    ["Privilege repair requires an explicitly unsafe current setting", "La corrección de privilegios requiere un ajuste actual explícitamente inseguro", "La correction des privilèges exige un réglage actuel explicitement non sûr", "Berechtigungskorrektur erfordert eine ausdrücklich unsichere aktuelle Einstellung", "A correção de privilégios exige uma configuração atual explicitamente insegura"],
    ["Privilege restore requires the current target setting; preference changed before restore", "Restaurar privilegios requiere el valor objetivo actual; la preferencia cambió antes de restaurar", "La restauration des privilèges exige le réglage cible actuel ; le réglage a changé avant la restauration", "Berechtigungswiederherstellung erfordert die aktuelle Zieleinstellung; Einstellung vor Wiederherstellung geändert", "Restaurar privilégios exige a configuração de destino atual; a preferência mudou antes da restauração"],
    ["Privilege registry readback did not match; mutation outcome requires review", "La relectura del registro de privilegios no coincide; revise el resultado del cambio", "La relecture du registre des privilèges ne correspond pas ; vérifiez le résultat de la modification", "Erneut gelesene Berechtigungsregistrierung stimmt nicht überein; Änderungsergebnis prüfen", "A releitura do registro de privilégios não corresponde; revise o resultado da alteração"],
    ["AutoAdminLogon is not a string", "AutoAdminLogon no es una cadena", "AutoAdminLogon n’est pas une chaîne", "AutoAdminLogon ist keine Zeichenfolge", "AutoAdminLogon não é uma cadeia de texto"],
    ["AutoAdminLogon has an unknown configuration", "AutoAdminLogon tiene una configuración desconocida", "AutoAdminLogon a une configuration inconnue", "AutoAdminLogon hat eine unbekannte Konfiguration", "AutoAdminLogon tem uma configuração desconhecida"],
    ["Automatic logon", "Inicio de sesión automático", "Connexion automatique", "Automatische Anmeldung", "Logon automático"],
    ["Preserving absent or already-safe machine preference", "Se conserva la preferencia del equipo ausente o ya segura", "Réglage machine absent ou déjà sûr conservé", "Fehlende oder bereits sichere Computereinstellung bleibt erhalten", "Preferência da máquina ausente ou já segura preservada"],
    ["AutoAdminLogon enabled=", "AutoAdminLogon habilitado=", "AutoAdminLogon activé=", "AutoAdminLogon aktiviert=", "AutoAdminLogon habilitado="],
    ["Winlogon DefaultPassword value present=", "Valor DefaultPassword de Winlogon presente=", "Valeur DefaultPassword de Winlogon présente=", "Winlogon-DefaultPassword-Wert vorhanden=", "Valor DefaultPassword do Winlogon presente="],
    ["Presence only: no password data is read. LSA-secret autologon storage is not inspected. Review physical access and credential exposure; automatic logon is preserved to avoid disrupting kiosk or sign-in workflows.", "Solo presencia: no se leen datos de contraseñas. No se inspecciona el almacenamiento de inicio automático en secretos LSA. Revise el acceso físico y la exposición de credenciales; se conserva el inicio automático para no interrumpir quioscos o procesos de acceso.", "Présence uniquement : aucune donnée de mot de passe n’est lue. Le stockage de connexion automatique dans les secrets LSA n’est pas inspecté. Vérifiez l’accès physique et l’exposition des identifiants ; la connexion automatique est conservée pour ne pas perturber les kiosques ou les usages de connexion.", "Nur Vorhandensein: keine Passwortdaten werden gelesen. Automatische Anmeldedaten in LSA-Geheimnissen werden nicht geprüft. Physischen Zugriff und Offenlegung von Anmeldedaten prüfen; automatische Anmeldung bleibt erhalten, um Kiosk- oder Anmeldeabläufe nicht zu beeinträchtigen.", "Apenas presença: nenhum dado de senha é lido. O armazenamento de logon automático em segredos LSA não é inspecionado. Revise o acesso físico e a exposição de credenciais; o logon automático é preservado para não interromper quiosques ou fluxos de entrada."],
    ["Relevant policy is configured or its authority is unknown: assessment only", "Hay una directiva relevante configurada o se desconoce su autoridad: solo evaluación", "Une stratégie pertinente est configurée ou son autorité est inconnue : évaluation uniquement", "Relevante Richtlinie ist konfiguriert oder ihre Zuständigkeit ist unbekannt: nur Bewertung", "Há uma política relevante configurada ou sua autoridade é desconhecida: apenas avaliação"],
    ["Group Policy authority is unknown: assessment only", "La autoridad de la directiva de grupo es desconocida: solo evaluación", "L’autorité de la stratégie de groupe est inconnue : évaluation uniquement", "Zuständigkeit der Gruppenrichtlinie ist unbekannt: nur Bewertung", "A autoridade da política de grupo é desconhecida: apenas avaliação"],
    ["Relevant resultant Group Policy: assessment only", "Directiva de grupo resultante relevante: solo evaluación", "Stratégie de groupe résultante pertinente : évaluation uniquement", "Relevante resultierende Gruppenrichtlinie: nur Bewertung", "Política de grupo resultante relevante: apenas avaliação"],
    ["Firewall preference/effective readback did not match; mutation outcome requires review", "La lectura posterior de la preferencia y del estado efectivo del cortafuegos no coincide; revise el resultado del cambio", "La relecture de la préférence et de l’état effectif du pare-feu ne correspond pas ; vérifiez le résultat de la modification", "Erneut gelesene Firewall-Einstellung bzw. wirksamer Zustand stimmt nicht überein; Änderungsergebnis prüfen", "A releitura da preferência e do estado efetivo do firewall não corresponde; revise o resultado da alteração"],
    ["No device-management registration or UAC policy authority found by the available probes. Each control repeats scoped policy and capability checks before mutation.", "Las comprobaciones disponibles no encontraron registro de administración del dispositivo ni autoridad de directivas UAC. Cada control repite las comprobaciones de directivas pertinentes y capacidades antes de modificar.", "Les vérifications disponibles n’ont trouvé ni inscription de gestion de l’appareil ni autorité de stratégie UAC. Chaque contrôle répète les vérifications de stratégies pertinentes et de capacités avant modification.", "Die verfügbaren Prüfungen fanden weder eine Geräteverwaltungsregistrierung noch eine UAC-Richtlinienzuständigkeit. Jede Kontrolle wiederholt vor Änderungen die relevanten Richtlinien- und Funktionsprüfungen.", "As verificações disponíveis não encontraram registro de gerenciamento do dispositivo nem autoridade de política UAC. Cada controle repete as verificações de políticas pertinentes e capacidades antes de alterar."],
    ["requires a JSON boolean", "requiere un booleano JSON", "exige un booléen JSON", "erfordert einen JSON-Wahrheitswert", "exige um booleano JSON"],
    ["MDM registration state is unknown", "El estado de registro MDM es desconocido", "L’état d’inscription MDM est inconnu", "MDM-Registrierungsstatus ist unbekannt", "O estado de registro MDM é desconhecido"],
    ["API result=", "resultado de API=", "résultat API=", "API-Ergebnis=", "resultado da API="],
    ["Device is registered with MDM: assessment only", "El dispositivo está registrado en MDM: solo evaluación", "L’appareil est inscrit à MDM : évaluation uniquement", "Gerät ist bei MDM registriert: nur Bewertung", "O dispositivo está registrado no MDM: apenas avaliação"],
    ["Probe returned an invalid finding", "La comprobación devolvió una observación no válida", "La vérification a renvoyé un constat non valide", "Prüfung lieferte einen ungültigen Befund", "A verificação retornou uma observação inválida"],
    ["Assessment unavailable", "Evaluación no disponible", "Évaluation indisponible", "Bewertung nicht verfügbar", "Avaliação indisponível"],
    ["Findings could not be collected", "No se pudieron recopilar las observaciones", "Impossible de recueillir les constats", "Befunde konnten nicht erfasst werden", "Não foi possível coletar as observações"],
    [" has incomplete apply or rollback; use revert to resolve its recorded preferences before applying again.", " tiene una aplicación o reversión incompleta; use revert para resolver sus preferencias registradas antes de aplicar de nuevo.", " a une application ou annulation incomplète ; utilisez revert pour résoudre ses préférences enregistrées avant de réappliquer.", " hat eine unvollständige Anwendung oder Rückabwicklung; mit revert die protokollierten Einstellungen vor erneuter Anwendung bereinigen.", " tem aplicação ou reversão incompleta; use revert para resolver suas preferências registradas antes de aplicar novamente."],
    ["Enable the core Defender preference only on an unmanaged device with no competing antivirus. Preserve exclusions and other preferences.", "Activar la preferencia principal de Defender solo en un dispositivo no administrado sin otro antivirus. Conservar exclusiones y otras preferencias.", "Activer la préférence principale de Defender uniquement sur un appareil non géré sans antivirus concurrent. Conserver les exclusions et autres préférences.", "Zentrale Defender-Einstellung nur auf unverwalteten Geräten ohne konkurrierenden Antivirus aktivieren. Ausschlüsse und andere Einstellungen beibehalten.", "Ativar a preferência principal do Defender apenas em dispositivo não gerenciado sem outro antivírus. Preservar exclusões e outras preferências."],
    ["Preserve all firewall rules and outbound policy. Unmanaged devices only.", "Conservar todas las reglas de cortafuegos y la política de salida. Solo dispositivos no administrados.", "Conserver toutes les règles de pare-feu et la stratégie sortante. Appareils non gérés uniquement.", "Alle Firewallregeln und ausgehenden Richtlinien beibehalten. Nur unverwaltete Geräte.", "Preservar todas as regras do firewall e a política de saída. Apenas dispositivos não gerenciados."],
    ["Repair an explicitly disabled EnableLUA value; preserve absent or already enabled settings.", "Reparar un valor EnableLUA explícitamente desactivado; conservar ajustes ausentes o ya activados.", "Réparer une valeur EnableLUA explicitement désactivée ; conserver les réglages absents ou déjà activés.", "Ausdrücklich deaktivierten EnableLUA-Wert reparieren; fehlende oder bereits aktivierte Einstellungen beibehalten.", "Reparar um valor EnableLUA explicitamente desativado; preservar configurações ausentes ou já ativadas."],
    ["Repair consent mode 0 to Windows default 5. Preserve every nonzero mode.", "Reparar el modo de consentimiento 0 al valor predeterminado 5 de Windows. Conservar todo modo distinto de cero.", "Remplacer le mode de consentement 0 par la valeur Windows par défaut 5. Conserver tout mode non nul.", "Zustimmungsmodus 0 auf Windows-Standard 5 korrigieren. Jeden Modus ungleich null beibehalten.", "Reparar o modo de consentimento 0 para o padrão 5 do Windows. Preservar todos os modos diferentes de zero."],
    ["A fixed local drive is required", "Se requiere una unidad local fija", "Un disque local fixe est requis", "Ein festes lokales Laufwerk ist erforderlich", "É necessária uma unidade local fixa"],
    ["Secblitz read-only security monitor", "Monitor de seguridad de solo lectura Secblitz", "Moniteur de sécurité Secblitz en lecture seule", "Schreibgeschützter Secblitz-Sicherheitsmonitor", "Monitor de segurança Secblitz somente leitura"],
    ["Read-only security observations every 15 minutes. No automatic remediation; latest report in Program Files/Secblitz/Monitor.", "Observaciones de seguridad de solo lectura cada 15 minutos. Sin reparaciones automáticas; último informe en Program Files/Secblitz/Monitor.", "Observations de sécurité en lecture seule toutes les 15 minutes. Aucune correction automatique ; dernier rapport dans Program Files/Secblitz/Monitor.", "Schreibgeschützte Sicherheitsbeobachtungen alle 15 Minuten. Keine automatische Reparatur; letzter Bericht in Program Files/Secblitz/Monitor.", "Observações de segurança somente leitura a cada 15 minutos. Sem correção automática; relatório mais recente em Program Files/Secblitz/Monitor."],
    ["Not applicable", "No aplicable", "Sans objet", "Nicht zutreffend", "Não aplicável"],
    ["Service exit code", "Código de salida del servicio", "Code de sortie du service", "Dienst-Exitcode", "Código de saída do serviço"],
    ["Checkpoint", "Punto de control", "Point de contrôle", "Prüfpunkt", "Ponto de controle"],
    ["Wait hint (ms)", "Espera estimada (ms)", "Attente estimée (ms)", "Geschätzte Wartezeit (ms)", "Espera estimada (ms)"],
    ["Expected an absolute local drive path", "Se esperaba una ruta absoluta de unidad local", "Chemin absolu de disque local attendu", "Absoluter lokaler Laufwerkspfad erwartet", "Era esperado um caminho absoluto de unidade local"],
    ["Invalid Windows path characters", "Caracteres de ruta Windows no válidos", "Caractères de chemin Windows non valides", "Ungültige Windows-Pfadzeichen", "Caracteres de caminho Windows inválidos"],
    ["Ambiguous Windows path component", "Componente de ruta Windows ambiguo", "Composant de chemin Windows ambigu", "Mehrdeutiger Windows-Pfadbestandteil", "Componente de caminho Windows ambíguo"],
    ["Reserved Windows device name", "Nombre de dispositivo Windows reservado", "Nom de périphérique Windows réservé", "Reservierter Windows-Gerätename", "Nome de dispositivo Windows reservado"],
    ["Embedded NUL", "NUL incrustado", "NUL intégré", "Eingebettetes NUL", "NUL embutido"],
    ["Security descriptor", "Descriptor de seguridad", "Descripteur de sécurité", "Sicherheitsbeschreibung", "Descritor de segurança"],
    ["Non-Unicode Windows path", "Ruta Windows no Unicode", "Chemin Windows non Unicode", "Windows-Pfad nicht in Unicode", "Caminho Windows não Unicode"],
    ["Cannot resolve Program Files", "No se puede resolver Program Files", "Impossible de résoudre Program Files", "Program Files kann nicht aufgelöst werden", "Não foi possível resolver Program Files"],
    ["File information", "Información de archivo", "Informations du fichier", "Dateiinformationen", "Informações do arquivo"],
    ["Reparse point rejected", "Punto de reanálisis rechazado", "Point de réanalyse rejeté", "Analysepunkt abgelehnt", "Ponto de nova análise rejeitado"],
    ["Wrong object type", "Tipo de objeto incorrecto", "Type d’objet incorrect", "Falscher Objekttyp", "Tipo de objeto incorreto"],
    ["Hard-linked file rejected", "Archivo con enlace duro rechazado", "Fichier à lien physique rejeté", "Datei mit harter Verknüpfung abgelehnt", "Arquivo com link físico rejeitado"],
    ["Cannot inspect file ACL", "No se puede inspeccionar la ACL del archivo", "Impossible d’inspecter l’ACL du fichier", "Datei-ACL kann nicht geprüft werden", "Não foi possível inspecionar a ACL do arquivo"],
    ["Cannot inspect owner", "No se puede inspeccionar el propietario", "Impossible d’inspecter le propriétaire", "Eigentümer kann nicht geprüft werden", "Não foi possível inspecionar o proprietário"],
    ["Cannot inspect DACL control", "No se puede inspeccionar el control de DACL", "Impossible d’inspecter le contrôle DACL", "DACL-Steuerung kann nicht geprüft werden", "Não foi possível inspecionar o controle de DACL"],
    ["Cannot inspect DACL", "No se puede inspeccionar la DACL", "Impossible d’inspecter la DACL", "DACL kann nicht geprüft werden", "Não foi possível inspecionar a DACL"],
    ["Untrusted file owner", "Propietario de archivo no confiable", "Propriétaire de fichier non fiable", "Nicht vertrauenswürdiger Dateieigentümer", "Proprietário de arquivo não confiável"],
    ["Missing/invalid DACL", "DACL ausente/no válida", "DACL absente/non valide", "Fehlende/ungültige DACL", "DACL ausente/inválida"],
    ["Unprotected DACL", "DACL sin protección", "DACL non protégée", "Ungeschützte DACL", "DACL desprotegida"],
    ["Cannot inspect ACE", "No se puede inspeccionar la ACE", "Impossible d’inspecter l’ACE", "ACE kann nicht geprüft werden", "Não foi possível inspecionar a ACE"],
    ["Short ACE", "ACE demasiado corta", "ACE trop courte", "Zu kurze ACE", "ACE curta demais"],
    ["Unsupported ACL entry", "Entrada de ACL no compatible", "Entrée ACL non prise en charge", "Nicht unterstützter ACL-Eintrag", "Entrada de ACL não suportada"],
    ["Unexpected ACE flags/type", "Indicadores/tipo de ACE inesperados", "Indicateurs/type ACE inattendus", "Unerwartete ACE-Flags/-Typ", "Indicadores/tipo de ACE inesperados"],
    ["Invalid trustee SID", "SID de entidad no válido", "SID de bénéficiaire non valide", "Ungültige Berechtigten-SID", "SID de entidade inválido"],
    ["Unexpected protected-object trustee", "Entidad de objeto protegido inesperada", "Bénéficiaire d’objet protégé inattendu", "Unerwarteter Berechtigter des geschützten Objekts", "Entidade de objeto protegido inesperada"],
    ["Unexpected protected-object rights", "Permisos de objeto protegido inesperados", "Droits d’objet protégé inattendus", "Unerwartete Rechte des geschützten Objekts", "Direitos de objeto protegido inesperados"],
    ["Missing ACL propagation", "Falta propagación de ACL", "Propagation ACL manquante", "ACL-Vererbung fehlt", "Propagação de ACL ausente"],
    ["Writable/untrusted ancestor DACL", "DACL de antecesor modificable/no confiable", "DACL parente modifiable/non fiable", "Beschreibbare/nicht vertrauenswürdige Vorgänger-DACL", "DACL ancestral gravável/não confiável"],
    ["Missing protected-object trustees", "Faltan entidades del objeto protegido", "Bénéficiaires d’objet protégé manquants", "Berechtigte des geschützten Objekts fehlen", "Entidades do objeto protegido ausentes"],
    ["Untrusted ancestor", "Antecesor no confiable", "Parent non fiable", "Nicht vertrauenswürdiger Vorgänger", "Ancestral não confiável"],
    ["Create directory failed", "Falló la creación del directorio", "Échec de création du dossier", "Verzeichniserstellung fehlgeschlagen", "Falha ao criar diretório"],
    ["Service installation requires Administrator elevation", "La instalación del servicio requiere elevación de administrador", "L’installation du service exige l’élévation administrateur", "Dienstinstallation erfordert Administratorrechte", "A instalação do serviço exige elevação de administrador"],
    ["The monitor requires Windows x64", "El monitor requiere Windows x64", "Le moniteur exige Windows x64", "Der Monitor benötigt Windows x64", "O monitor exige Windows x64"],
    ["SecblitzMonitor already exists; it was not changed", "SecblitzMonitor ya existe; no se modificó", "SecblitzMonitor existe déjà ; il n’a pas été modifié", "SecblitzMonitor ist bereits vorhanden; wurde nicht geändert", "SecblitzMonitor já existe; não foi alterado"],
    ["Invalid installer executable", "Ejecutable de instalación no válido", "Exécutable d’installation non valide", "Ungültige Installer-Programmdatei", "Executável de instalação inválido"],
    ["Destination binary exists and is not this installer; refusing overwrite", "El ejecutable de destino existe y no es este instalador; se rechaza sobrescribir", "L’exécutable cible existe et n’est pas cet installeur ; écrasement refusé", "Zielprogrammdatei existiert und ist nicht dieser Installer; Überschreiben verweigert", "O executável de destino existe e não é este instalador; sobrescrita recusada"],
    ["Cannot secure service configuration", "No se puede proteger la configuración del servicio", "Impossible de protéger la configuration du service", "Dienstkonfiguration kann nicht abgesichert werden", "Não foi possível proteger a configuração do serviço"],
    ["Cannot set service command", "No se puede configurar el comando del servicio", "Impossible de définir la commande du service", "Dienstbefehl kann nicht festgelegt werden", "Não foi possível definir o comando do serviço"],
    ["Cannot restrict service privileges", "No se pueden restringir los privilegios del servicio", "Impossible de restreindre les privilèges du service", "Dienstprivilegien können nicht eingeschränkt werden", "Não foi possível restringir os privilégios do serviço"],
    ["Cannot enable automatic startup", "No se puede activar el inicio automático", "Impossible d’activer le démarrage automatique", "Automatischer Start kann nicht aktiviert werden", "Não foi possível ativar a inicialização automática"],
    ["Install failed", "Falló la instalación", "Échec de l’installation", "Installation fehlgeschlagen", "Falha na instalação"],
    ["rollback could not remove registration; files retained", "la reversión no pudo eliminar el registro; se conservaron los archivos", "l’annulation n’a pas pu supprimer l’inscription ; fichiers conservés", "Rückabwicklung konnte Registrierung nicht entfernen; Dateien beibehalten", "a reversão não removeu o registro; arquivos mantidos"],
    ["Installation rolled back; cleanup failures", "Instalación revertida; fallos de limpieza", "Installation annulée ; échecs de nettoyage", "Installation zurückgenommen; Bereinigungsfehler", "Instalação revertida; falhas na limpeza"],
    ["Service removal requires Administrator elevation", "La eliminación del servicio requiere elevación de administrador", "La suppression du service exige l’élévation administrateur", "Dienstentfernung erfordert Administratorrechte", "A remoção do serviço exige elevação de administrador"],
    ["Unexpected service configuration; refusing to delete", "Configuración de servicio inesperada; se rechaza eliminar", "Configuration de service inattendue ; suppression refusée", "Unerwartete Dienstkonfiguration; Löschen verweigert", "Configuração de serviço inesperada; exclusão recusada"],
    ["Stop SecblitzMonitor through SCM before uninstalling", "Detenga SecblitzMonitor mediante SCM antes de desinstalar", "Arrêtez SecblitzMonitor via SCM avant la désinstallation", "SecblitzMonitor vor Deinstallation über SCM beenden", "Pare SecblitzMonitor pelo SCM antes de desinstalar"],
    ["Monitor worker panicked", "El proceso de supervisión falló inesperadamente", "Le processus de surveillance a échoué de manière inattendue", "Monitor-Arbeitsthread ist unerwartet abgebrochen", "O processo de monitoramento falhou inesperadamente"],
    ["Monitor report exceeded limit", "El informe del monitor superó el límite", "Le rapport du moniteur a dépassé la limite", "Monitorbericht überschritt das Limit", "O relatório do monitor excedeu o limite"],
    ["Report limit", "Límite de informe", "Limite du rapport", "Berichtslimit", "Limite de relatório"],
    ["Cannot inspect journal handle", "No se puede inspeccionar el identificador del diario", "Impossible d’inspecter le handle du journal", "Journalhandle kann nicht geprüft werden", "Não foi possível inspecionar o identificador do diário"],
    ["Cannot inspect journal identity", "No se puede inspeccionar la identidad del diario", "Impossible d’inspecter l’identité du journal", "Journalidentität kann nicht geprüft werden", "Não foi possível inspecionar a identidade do diário"],
    ["Non-UTF8 journal filename", "Nombre de archivo del diario no UTF-8", "Nom de fichier journal non UTF-8", "Journaldateiname nicht in UTF-8", "Nome de arquivo de diário não UTF-8"],
    ["Journal schema, machine, or transaction identity mismatch", "No coinciden el esquema, equipo o identidad de transacción del diario", "Schéma, machine ou identité de transaction du journal incohérents", "Journalschema, Computer- oder Transaktionsidentität stimmt nicht überein", "Esquema, máquina ou identidade de transação do diário não corresponde"],
    ["The protected journal directory is only available on Windows", "El directorio de diario protegido solo está disponible en Windows", "Le dossier journal protégé est disponible uniquement sous Windows", "Das geschützte Journalverzeichnis ist nur unter Windows verfügbar", "O diretório de diário protegido só está disponível no Windows"],
    ["Embedded NUL in Windows string", "NUL incrustado en cadena de Windows", "NUL intégré dans une chaîne Windows", "Eingebettetes NUL in Windows-Zeichenfolge", "NUL embutido em cadeia do Windows"],
    ["Cannot resolve trusted Windows directory", "No se puede resolver el directorio de Windows de confianza", "Impossible de résoudre le dossier Windows de confiance", "Vertrauenswürdiges Windows-Verzeichnis kann nicht aufgelöst werden", "Não foi possível resolver o diretório confiável do Windows"],
    ["Windows directory is not absolute", "El directorio de Windows no es absoluto", "Le dossier Windows n’est pas absolu", "Windows-Verzeichnis ist nicht absolut", "O diretório do Windows não é absoluto"],
    ["Invalid elevation information", "Información de elevación no válida", "Informations d’élévation non valides", "Ungültige Informationen zur Rechteerhöhung", "Informações de elevação inválidas"],
    ["Elevation was cancelled or failed", "La elevación se canceló o falló", "L’élévation a été annulée ou a échoué", "Rechteerhöhung abgebrochen oder fehlgeschlagen", "A elevação foi cancelada ou falhou"],
    ["ShellExecute code", "Código ShellExecute", "Code ShellExecute", "ShellExecute-Code", "Código ShellExecute"],
    ["Cannot create bounded process job", "No se puede crear el trabajo de proceso limitado", "Impossible de créer la tâche de processus limitée", "Begrenzter Prozessauftrag kann nicht erstellt werden", "Não foi possível criar a tarefa de processo limitada"],
    ["Secblitz supports Windows x64 only", "Secblitz solo admite Windows x64", "Secblitz prend en charge uniquement Windows x64", "Secblitz unterstützt nur Windows x64", "Secblitz suporta apenas Windows x64"],
    ["Unknown platform action", "Acción de plataforma desconocida", "Action de plateforme inconnue", "Unbekannte Plattformaktion", "Ação de plataforma desconhecida"],
    ["Missing control id", "Falta el identificador del control", "Identifiant de contrôle manquant", "Kontrollkennung fehlt", "Identificador de controle ausente"],
    ["Start inbox Windows PowerShell", "Iniciar Windows PowerShell integrado", "Démarrer Windows PowerShell intégré", "Integriertes Windows PowerShell starten", "Iniciar Windows PowerShell integrado"],
    ["Assign PowerShell job", "Asignar trabajo de PowerShell", "Associer la tâche PowerShell", "PowerShell-Auftrag zuordnen", "Associar tarefa do PowerShell"],
    ["Missing stdin", "Falta stdin", "stdin manquant", "stdin fehlt", "stdin ausente"],
    ["Missing stdout", "Falta stdout", "stdout manquant", "stdout fehlt", "stdout ausente"],
    ["Missing stderr", "Falta stderr", "stderr manquant", "stderr fehlt", "stderr ausente"],
    ["PowerShell timed out; mutation outcome may be unknown", "PowerShell agotó el tiempo; el resultado del cambio puede ser desconocido", "Délai PowerShell dépassé ; le résultat de la modification peut être inconnu", "PowerShell-Zeitlimit überschritten; Änderungsergebnis möglicherweise unbekannt", "O PowerShell excedeu o tempo; o resultado da alteração pode ser desconhecido"],
    ["PowerShell output timeout/disconnect; mutation outcome may be unknown", "Tiempo agotado/desconexión de salida de PowerShell; el resultado del cambio puede ser desconocido", "Délai dépassé/déconnexion de sortie PowerShell ; le résultat de la modification peut être inconnu", "PowerShell-Ausgabezeitlimit/Verbindungsabbruch; Änderungsergebnis möglicherweise unbekannt", "Tempo excedido/desconexão da saída do PowerShell; o resultado da alteração pode ser desconhecido"],
    ["PowerShell exceeded 2 MiB output limit; mutation outcome may be unknown", "PowerShell superó el límite de salida de 2 MiB; el resultado del cambio puede ser desconocido", "PowerShell a dépassé la limite de sortie de 2 MiB ; le résultat de la modification peut être inconnu", "PowerShell überschritt das Ausgabelimit von 2 MiB; Änderungsergebnis möglicherweise unbekannt", "O PowerShell excedeu o limite de saída de 2 MiB; o resultado da alteração pode ser desconhecido"],
    ["PowerShell exit timed out; mutation outcome may be unknown", "Tiempo agotado al finalizar PowerShell; el resultado del cambio puede ser desconocido", "Délai de fin PowerShell dépassé ; le résultat de la modification peut être inconnu", "PowerShell-Beendigung überschritt Zeitlimit; Änderungsergebnis möglicherweise unbekannt", "Tempo excedido ao encerrar PowerShell; o resultado da alteração pode ser desconhecido"],
    ["PowerShell failed", "Falló PowerShell", "Échec de PowerShell", "PowerShell fehlgeschlagen", "Falha no PowerShell"],
    ["PowerShell input writer failed", "Falló el escritor de entrada de PowerShell", "Échec d’écriture de l’entrée PowerShell", "PowerShell-Eingabeschreiber fehlgeschlagen", "Falha na escrita da entrada do PowerShell"],
    ["Invalid PowerShell response (no result assumed)", "Respuesta de PowerShell no válida (no se presupone ningún resultado)", "Réponse PowerShell non valide (aucun résultat présumé)", "Ungültige PowerShell-Antwort (kein Ergebnis angenommen)", "Resposta do PowerShell inválida (nenhum resultado presumido)"],
    ["Inbox Windows PowerShell is unavailable", "Windows PowerShell integrado no está disponible", "Windows PowerShell intégré est indisponible", "Integriertes Windows PowerShell ist nicht verfügbar", "Windows PowerShell integrado indisponível"],
    ["Invalid Windows machine identity", "Identidad de equipo Windows no válida", "Identité de machine Windows non valide", "Ungültige Windows-Computeridentität", "Identidade de máquina Windows inválida"],
    ["Administrator elevation is required", "Se requiere elevación de administrador", "L’élévation administrateur est requise", "Administratorrechte sind erforderlich", "É necessária elevação de administrador"],
    ["Write was not acknowledged", "No se confirmó la escritura", "L’écriture n’a pas été confirmée", "Schreibvorgang wurde nicht bestätigt", "A gravação não foi confirmada"],
    ["Preference readback did not match; mutation outcome requires review", "La lectura posterior de la preferencia no coincide; revise el resultado del cambio", "La relecture de la préférence ne correspond pas ; vérifiez le résultat de la modification", "Erneut gelesene Einstellung stimmt nicht überein; Änderungsergebnis prüfen", "A releitura da preferência não corresponde; revise o resultado da alteração"],
    ["Cannot open journal path safely", "No se puede abrir la ruta del diario de forma segura", "Impossible d’ouvrir le chemin du journal en sécurité", "Journalpfad kann nicht sicher geöffnet werden", "Não foi possível abrir o caminho do diário com segurança"],
    ["Journal path contains a reparse point", "La ruta del diario contiene un punto de reanálisis", "Le chemin du journal contient un point de réanalyse", "Journalpfad enthält einen Analysepunkt", "O caminho do diário contém um ponto de nova análise"],
    ["Journal file has multiple hard links", "El archivo del diario tiene varios enlaces duros", "Le fichier journal a plusieurs liens physiques", "Journaldatei hat mehrere harte Verknüpfungen", "O arquivo do diário tem vários links físicos"],
    ["Cannot query journal security", "No se puede consultar la seguridad del diario", "Impossible d’interroger la sécurité du journal", "Journalsicherheit kann nicht abgefragt werden", "Não foi possível consultar a segurança do diário"],
    ["Windows error", "Error de Windows", "Erreur Windows", "Windows-Fehler", "Erro do Windows"],
    ["Journal path owner is not SYSTEM or Administrators", "El propietario de la ruta del diario no es SYSTEM ni Administrators", "Le propriétaire du chemin du journal n’est ni SYSTEM ni Administrators", "Journalpfadeigentümer ist weder SYSTEM noch Administrators", "O proprietário do caminho do diário não é SYSTEM nem Administrators"],
    ["Journal DACL is missing or invalid", "Falta la DACL del diario o no es válida", "DACL du journal absente ou non valide", "Journal-DACL fehlt oder ist ungültig", "A DACL do diário está ausente ou inválida"],
    ["Journal DACL is not present", "La DACL del diario no está presente", "DACL du journal absente", "Journal-DACL nicht vorhanden", "A DACL do diário não está presente"],
    ["Journal root DACL permits inheritance", "La DACL raíz del diario permite herencia", "La DACL racine du journal permet l’héritage", "Journalstamm-DACL erlaubt Vererbung", "A DACL raiz do diário permite herança"],
    ["Unexpected journal ACE type", "Tipo de ACE del diario inesperado", "Type d’ACE du journal inattendu", "Unerwarteter Journal-ACE-Typ", "Tipo de ACE do diário inesperado"],
    ["Unexpected journal ACE flags", "Indicadores de ACE del diario inesperados", "Indicateurs d’ACE du journal inattendus", "Unerwartete Journal-ACE-Flags", "Indicadores de ACE do diário inesperados"],
    ["Journal directory does not propagate its restricted DACL", "El directorio del diario no propaga su DACL restringida", "Le dossier journal ne propage pas sa DACL restreinte", "Journalverzeichnis vererbt seine eingeschränkte DACL nicht", "O diretório do diário não propaga sua DACL restrita"],
    ["Unexpected journal access mask", "Máscara de acceso del diario inesperada", "Masque d’accès du journal inattendu", "Unerwartete Journalzugriffsmaske", "Máscara de acesso do diário inesperada"],
    ["Invalid journal trustee SID", "SID de entidad del diario no válido", "SID du bénéficiaire du journal non valide", "Ungültige SID des Journalberechtigten", "SID da entidade do diário inválido"],
    ["Untrusted journal trustee", "Entidad del diario no confiable", "Bénéficiaire du journal non fiable", "Nicht vertrauenswürdiger Journalberechtigter", "Entidade do diário não confiável"],
    ["Journal must grant full control to SYSTEM and Administrators", "El diario debe conceder control total a SYSTEM y Administrators", "Le journal doit accorder le contrôle total à SYSTEM et Administrators", "Journal muss SYSTEM und Administrators Vollzugriff gewähren", "O diário deve conceder controle total a SYSTEM e Administrators"],
    ["Cannot resolve ProgramData known folder", "No se puede resolver la carpeta conocida ProgramData", "Impossible de résoudre le dossier connu ProgramData", "Bekannter Ordner ProgramData kann nicht aufgelöst werden", "Não foi possível resolver a pasta conhecida ProgramData"],
    ["Invalid known-folder path", "Ruta de carpeta conocida no válida", "Chemin du dossier connu non valide", "Ungültiger Pfad des bekannten Ordners", "Caminho de pasta conhecida inválido"],
    ["Journal directory exceeds inspection limits", "El directorio del diario supera los límites de inspección", "Le dossier journal dépasse les limites d’inspection", "Journalverzeichnis überschreitet Prüflimits", "O diretório do diário excede os limites de inspeção"],
    ["Inspect", "Inspeccionar", "Inspecter", "Prüfen", "Inspecionar"],
    ["Untrusted journal entry", "Entrada de diario no confiable", "Entrée du journal non fiable", "Nicht vertrauenswürdiger Journaleintrag", "Entrada de diário não confiável"],
    ["Too many journal entries", "Demasiadas entradas de diario", "Trop d’entrées dans le journal", "Zu viele Journaleinträge", "Entradas demais no diário"],
    ["Journal lock poisoned", "Bloqueo del diario invalidado", "Verrou du journal invalidé", "Journalsperre beschädigt", "Bloqueio do diário invalidado"],
    ["Protected journal access requires Administrator elevation", "El acceso al diario protegido requiere elevación de administrador", "L’accès au journal protégé exige l’élévation administrateur", "Zugriff auf geschütztes Journal erfordert Administratorrechte", "O acesso ao diário protegido exige elevação de administrador"],
    ["ProgramData must be on a local drive", "ProgramData debe estar en una unidad local", "ProgramData doit être sur un disque local", "ProgramData muss auf einem lokalen Laufwerk liegen", "ProgramData deve estar em uma unidade local"],
    ["ProgramData is not absolute", "ProgramData no es absoluto", "ProgramData n’est pas absolu", "ProgramData ist nicht absolut", "ProgramData não é absoluto"],
    ["Noncanonical ProgramData path", "Ruta ProgramData no canónica", "Chemin ProgramData non canonique", "Nichtkanonischer ProgramData-Pfad", "Caminho ProgramData não canônico"],
    ["Cannot inspect volume root", "No se puede inspeccionar la raíz del volumen", "Impossible d’inspecter la racine du volume", "Volumestamm kann nicht geprüft werden", "Não foi possível inspecionar a raiz do volume"],
    ["Invalid volume root", "Raíz de volumen no válida", "Racine de volume non valide", "Ungültiger Volumestamm", "Raiz de volume inválida"],
    ["Journal requires a fixed local drive", "El diario requiere una unidad local fija", "Le journal exige un disque local fixe", "Journal benötigt ein festes lokales Laufwerk", "O diário exige uma unidade local fixa"],
    ["ProgramData ancestor is not a directory", "Un antecesor de ProgramData no es un directorio", "Un parent de ProgramData n’est pas un dossier", "ProgramData-Vorgänger ist kein Verzeichnis", "Um ancestral de ProgramData não é um diretório"],
    ["Cannot construct journal security descriptor", "No se puede crear el descriptor de seguridad del diario", "Impossible de construire le descripteur de sécurité du journal", "Journal-Sicherheitsbeschreibung kann nicht erstellt werden", "Não foi possível criar o descritor de segurança do diário"],
    ["Cannot create protected journal directory", "No se puede crear el directorio de diario protegido", "Impossible de créer le dossier journal protégé", "Geschütztes Journalverzeichnis kann nicht erstellt werden", "Não foi possível criar o diretório de diário protegido"],
    ["Journal root is not a directory", "La raíz del diario no es un directorio", "La racine du journal n’est pas un dossier", "Journalstamm ist kein Verzeichnis", "A raiz do diário não é um diretório"],
    ["ProgramData changed during this process", "ProgramData cambió durante este proceso", "ProgramData a changé durant ce processus", "ProgramData wurde während dieses Prozesses geändert", "ProgramData mudou durante este processo"],
    ["Unknown control id", "Identificador de control desconocido", "Identifiant de contrôle inconnu", "Unbekannte Kontrollkennung", "Identificador de controle desconhecido"],
    ["Unknown control", "Control desconocido", "Contrôle inconnu", "Unbekannte Kontrolle", "Controle desconhecido"],
    ["Unknown operation", "Operación desconocida", "Opération inconnue", "Unbekannter Vorgang", "Operação desconhecida"],
    ["Invalid MachineGuid", "MachineGuid no válido", "MachineGuid non valide", "Ungültige MachineGuid", "MachineGuid inválido"],
    ["Defender tamper state changed or is unknown; mutation outcome requires review", "El estado de protección contra alteraciones de Defender cambió o es desconocido; revise el resultado del cambio", "L’état de protection contre les falsifications de Defender a changé ou est inconnu ; vérifiez le résultat de la modification", "Defender-Manipulationsschutzstatus geändert oder unbekannt; Änderungsergebnis prüfen", "O estado da proteção contra adulteração do Defender mudou ou é desconhecido; revise o resultado da alteração"],
    ["Defender became unavailable or passive; mutation outcome requires review", "Defender dejó de estar disponible o pasó a modo pasivo; revise el resultado del cambio", "Defender est devenu indisponible ou passif ; vérifiez le résultat de la modification", "Defender wurde nicht verfügbar oder passiv; Änderungsergebnis prüfen", "O Defender ficou indisponível ou passivo; revise o resultado da alteração"],
    ["Defender preference/runtime readback did not match; mutation outcome requires review", "La lectura posterior de preferencia/estado de Defender no coincide; revise el resultado del cambio", "La relecture des préférences/de l’état de Defender ne correspond pas ; vérifiez le résultat de la modification", "Defender-Einstellung/Laufzeitstatus stimmt nach erneutem Lesen nicht überein; Änderungsergebnis prüfen", "A releitura da preferência/estado do Defender não corresponde; revise o resultado da alteração"],
    ["UAC restore requires the current target setting; preference changed before restore", "Restaurar UAC requiere el valor objetivo actual; la preferencia cambió antes de restaurar", "La restauration UAC exige le réglage cible actuel ; la préférence a changé avant la restauration", "UAC-Wiederherstellung erfordert die aktuelle Zieleinstellung; Einstellung vor Wiederherstellung geändert", "Restaurar UAC exige a configuração de destino atual; a preferência mudou antes da restauração"],
    ["Invalid boolean preference for", "Preferencia booleana no válida para", "Préférence booléenne non valide pour", "Ungültige boolesche Einstellung für", "Preferência booleana inválida para"],
    ["Invalid inbound preference", "Preferencia de entrada no válida", "Préférence entrante non valide", "Ungültige Eingangseinstellung", "Preferência de entrada inválida"],
    ["Invalid inbound action", "Acción de entrada no válida", "Action entrante non valide", "Ungültige Eingangsaktion", "Ação de entrada inválida"],
    ["Invalid UAC preference", "Preferencia UAC no válida", "Préférence UAC non valide", "Ungültige UAC-Einstellung", "Preferência UAC inválida"],
    ["Invalid UAC fields", "Campos UAC no válidos", "Champs UAC non valides", "Ungültige UAC-Felder", "Campos UAC inválidos"],
    ["Absent UAC preference must have null value", "La preferencia UAC ausente debe tener valor nulo", "Une préférence UAC absente doit avoir une valeur nulle", "Fehlende UAC-Einstellung muss Nullwert haben", "A preferência UAC ausente deve ter valor nulo"],
    ["Invalid UAC DWORD", "DWORD de UAC no válido", "DWORD UAC non valide", "Ungültiger UAC-DWORD-Wert", "DWORD de UAC inválido"],
    ["Invalid UAC presence flag", "Indicador de presencia UAC no válido", "Indicateur de présence UAC non valide", "Ungültiges UAC-Vorhandenseinsflag", "Indicador de presença UAC inválido"],
    ["Registry state must contain exactly present and value", "El estado del registro debe contener exactamente present y value", "L’état du registre doit contenir exactement present et value", "Registrierungszustand muss genau present und value enthalten", "O estado do registro deve conter exatamente present e value"],
    ["Absent registry value must be null", "El valor de registro ausente debe ser nulo", "Une valeur de registre absente doit être nulle", "Fehlender Registrierungswert muss null sein", "O valor de registro ausente deve ser nulo"],
    ["Journal links are forbidden", "Los enlaces del diario están prohibidos", "Les liens du journal sont interdits", "Journalverknüpfungen sind unzulässig", "Links no diário são proibidos"],
    ["Unexpected journal file type", "Tipo de archivo de diario inesperado", "Type de fichier journal inattendu", "Unerwarteter Journaldateityp", "Tipo de arquivo de diário inesperado"],
    ["Journal hard links are forbidden", "Los enlaces duros del diario están prohibidos", "Les liens physiques du journal sont interdits", "Harte Journalverknüpfungen sind unzulässig", "Links físicos no diário são proibidos"],
    ["Journal reparse points are forbidden", "Los puntos de reanálisis del diario están prohibidos", "Les points de réanalyse du journal sont interdits", "Journal-Analysepunkte sind unzulässig", "Pontos de nova análise no diário são proibidos"],
    ["Cannot inspect journal file", "No se puede inspeccionar el archivo de diario", "Impossible d’inspecter le fichier journal", "Journaldatei kann nicht geprüft werden", "Não foi possível inspecionar o arquivo de diário"],
    ["Open journal", "Abrir diario", "Ouvrir le journal", "Journal öffnen", "Abrir diário"],
    ["Journal file identity changed", "La identidad del archivo de diario cambió", "L’identité du fichier journal a changé", "Identität der Journaldatei geändert", "A identidade do arquivo de diário mudou"],
    ["Engine requires the protected platform journal directory", "El motor requiere el directorio de diario protegido de la plataforma", "Le moteur exige le dossier journal protégé de la plateforme", "Engine benötigt das geschützte Plattform-Journalverzeichnis", "O mecanismo exige o diretório de diário protegido da plataforma"],
    ["Duplicate backend control", "Control de motor duplicado", "Contrôle du moteur en double", "Doppelte Backend-Kontrolle", "Controle do mecanismo duplicado"],
    ["Backend target differs from compiled target", "El objetivo del motor difiere del objetivo compilado", "La cible du moteur diffère de la cible compilée", "Backend-Ziel weicht vom einkompilierten Ziel ab", "O destino do mecanismo difere do destino compilado"],
    ["Invalid machine identity", "Identidad de equipo no válida", "Identité de machine non valide", "Ungültige Computeridentität", "Identidade da máquina inválida"],
    ["Journal storage failed; reopen the engine after resolving storage failure", "Falló el almacenamiento del diario; vuelva a abrir el motor tras resolver el fallo", "Échec du stockage du journal ; rouvrez le moteur après résolution du problème", "Journalspeicherung fehlgeschlagen; Engine nach Behebung des Speicherfehlers erneut öffnen", "Falha no armazenamento do diário; reabra o mecanismo após resolver a falha"],
    ["Another Secblitz operation holds the journal lock", "Otra operación de Secblitz mantiene el bloqueo del diario", "Une autre opération Secblitz détient le verrou du journal", "Ein anderer Secblitz-Vorgang hält die Journalsperre", "Outra operação do Secblitz mantém o bloqueio do diário"],
    ["Journal control is not supported by this backend", "Este motor no admite el control del diario", "Ce moteur ne prend pas en charge le contrôle du journal", "Dieses Backend unterstützt die Journalkontrolle nicht", "Este mecanismo não suporta o controle do diário"],
    ["Too many journal transactions", "Demasiadas transacciones en el diario", "Trop de transactions dans le journal", "Zu viele Journaltransaktionen", "Transações demais no diário"],
    ["Unexpected journal entry", "Entrada de diario inesperada", "Entrée de journal inattendue", "Unerwarteter Journaleintrag", "Entrada de diário inesperada"],
    ["Invalid journal filename", "Nombre de archivo de diario no válido", "Nom de fichier journal non valide", "Ungültiger Journaldateiname", "Nome de arquivo de diário inválido"],
    ["Invalid journal sequence", "Secuencia de diario no válida", "Séquence du journal non valide", "Ungültige Journalsequenz", "Sequência de diário inválida"],
    ["Invalid transaction UUID", "UUID de transacción no válido", "UUID de transaction non valide", "Ungültige Transaktions-UUID", "UUID de transação inválido"],
    ["Journal exceeds size limit", "El diario supera el límite de tamaño", "Le journal dépasse la taille limite", "Journal überschreitet Größenlimit", "O diário excede o limite de tamanho"],
    ["Empty, oversized, or truncated journal", "Diario vacío, demasiado grande o truncado", "Journal vide, trop volumineux ou tronqué", "Leeres, übergroßes oder abgeschnittenes Journal", "Diário vazio, grande demais ou truncado"],
    ["Invalid journal record size", "Tamaño de registro del diario no válido", "Taille d’entrée du journal non valide", "Ungültige Journaldatensatzgröße", "Tamanho de registro do diário inválido"],
    ["Invalid journal record", "Registro del diario no válido", "Entrée du journal non valide", "Ungültiger Journaldatensatz", "Registro do diário inválido"],
    ["Missing header", "Falta la cabecera", "En-tête manquant", "Kopfzeile fehlt", "Cabeçalho ausente"],
    ["Journal must start with a header", "El diario debe empezar con una cabecera", "Le journal doit commencer par un en-tête", "Journal muss mit einer Kopfzeile beginnen", "O diário deve começar com um cabeçalho"],
    ["Records after transaction completion", "Registros tras completar la transacción", "Entrées après la fin de transaction", "Datensätze nach Transaktionsabschluss", "Registros após conclusão da transação"],
    ["Redundant before image", "Estado anterior redundante", "État antérieur redondant", "Redundantes Vorabbild", "Estado anterior redundante"],
    ["Invalid prepare ordering or duplicate before image", "Orden de preparación no válido o estado anterior duplicado", "Ordre de préparation non valide ou état antérieur en double", "Ungültige Vorbereitungsreihenfolge oder doppeltes Vorabbild", "Ordem de preparação inválida ou estado anterior duplicado"],
    ["Apply after seal/revert", "Aplicación después del cierre/reversión", "Application après clôture/annulation", "Anwendung nach Versiegelung/Zurücksetzen", "Aplicação após fechamento/reversão"],
    ["Apply without prepare", "Aplicación sin preparación", "Application sans préparation", "Anwendung ohne Vorbereitung", "Aplicação sem preparação"],
    ["Invalid apply result", "Resultado de aplicación no válido", "Résultat d’application non valide", "Ungültiges Anwendungsergebnis", "Resultado de aplicação inválido"],
    ["Invalid seal", "Cierre no válido", "Clôture non valide", "Ungültige Versiegelung", "Fechamento inválido"],
    ["Duplicate revert start", "Inicio de reversión duplicado", "Début d’annulation en double", "Doppelter Rücksetzbeginn", "Início de reversão duplicado"],
    ["Restore before revert start", "Restauración antes del inicio de reversión", "Restauration avant le début d’annulation", "Wiederherstellung vor Rücksetzbeginn", "Restauração antes do início da reversão"],
    ["Restore without before image", "Restauración sin estado anterior", "Restauration sans état antérieur", "Wiederherstellung ohne Vorabbild", "Restauração sem estado anterior"],
    ["Restore after completion", "Restauración después de completar", "Restauration après la fin", "Wiederherstellung nach Abschluss", "Restauração após conclusão"],
    ["Restore result before revert start", "Resultado de restauración antes del inicio de reversión", "Résultat de restauration avant le début d’annulation", "Wiederherstellungsergebnis vor Rücksetzbeginn", "Resultado de restauração antes do início da reversão"],
    ["Result without before image", "Resultado sin estado anterior", "Résultat sans état antérieur", "Ergebnis ohne Vorabbild", "Resultado sem estado anterior"],
    ["Restore result without intent", "Resultado de restauración sin intención registrada", "Résultat de restauration sans intention enregistrée", "Wiederherstellungsergebnis ohne protokollierte Absicht", "Resultado de restauração sem intenção registrada"],
    ["Premature revert completion", "Finalización prematura de reversión", "Fin d’annulation prématurée", "Vorzeitiger Rücksetzabschluss", "Conclusão prematura da reversão"],
    ["Duplicate journal header", "Cabecera de diario duplicada", "En-tête de journal en double", "Doppelte Journalkopfzeile", "Cabeçalho de diário duplicado"],
    ["Duplicate transaction sequence", "Secuencia de transacción duplicada", "Séquence de transaction en double", "Doppelte Transaktionssequenz", "Sequência de transação duplicada"],
    ["Journal length changed since validation", "La longitud del diario cambió desde la validación", "La longueur du journal a changé depuis la validation", "Journallänge seit Validierung geändert", "O comprimento do diário mudou desde a validação"],
    ["Journal record exceeds limit", "El registro del diario supera el límite", "L’entrée du journal dépasse la limite", "Journaldatensatz überschreitet Limit", "O registro do diário excede o limite"],
    ["Journal is full", "El diario está lleno", "Le journal est plein", "Journal ist voll", "O diário está cheio"],
    ["Transaction sequence exhausted", "Secuencia de transacciones agotada", "Séquence de transactions épuisée", "Transaktionssequenz erschöpft", "Sequência de transações esgotada"],
    ["changed or became ineligible after prepare; revert transaction", "cambió o dejó de ser elegible tras la preparación; revierta la transacción", "a changé ou n’est plus admissible après préparation ; annulez la transaction", "wurde nach Vorbereitung geändert oder unzulässig; Transaktion zurücksetzen", "mudou ou deixou de ser elegível após preparação; reverta a transação"],
    ["has unknown outcome; pending transaction", "tiene resultado desconocido; transacción pendiente", "a un résultat inconnu ; transaction en attente", "hat unbekanntes Ergebnis; ausstehende Transaktion", "tem resultado desconhecido; transação pendente"],
    ["retained", "conservada", "conservée", "beibehalten", "mantida"],
    ["Apply", "Aplicar", "Appliquer", "Anwenden", "Aplicar"],
    ["Restore", "Restaurar", "Restaurer", "Wiederherstellen", "Restaurar"],
    ["Installed SecblitzMonitor as LocalService; it has not been started. Binary and reports are retained on uninstall.", "SecblitzMonitor instalado como LocalService; no se ha iniciado. El ejecutable y los informes se conservan al desinstalar.", "SecblitzMonitor installé sous LocalService ; il n’a pas été démarré. L’exécutable et les rapports sont conservés à la désinstallation.", "SecblitzMonitor als LocalService installiert; noch nicht gestartet. Programmdatei und Berichte bleiben bei der Deinstallation erhalten.", "SecblitzMonitor instalado como LocalService; não foi iniciado. O executável e os relatórios são mantidos na desinstalação."],
    ["SecblitzMonitor registration is removed or was already absent. Binary, reports and journals are preserved.", "El registro de SecblitzMonitor se eliminó o ya estaba ausente. Se conservan el ejecutable, los informes y los diarios.", "L’inscription de SecblitzMonitor a été supprimée ou était déjà absente. L’exécutable, les rapports et les journaux sont conservés.", "SecblitzMonitor-Registrierung entfernt oder bereits nicht vorhanden. Programmdatei, Berichte und Journale bleiben erhalten.", "O registro do SecblitzMonitor foi removido ou já estava ausente. O executável, os relatórios e os diários são preservados."],
    ["Service diagnostic (native codes and values)", "Diagnóstico del servicio (códigos y valores nativos)", "Diagnostic du service (codes et valeurs natifs)", "Dienstdiagnose (native Codes und Werte)", "Diagnóstico do serviço (códigos e valores nativos)"],
    ["Password generation failed", "Falló la generación de la contraseña", "Échec de génération du mot de passe", "Passworterzeugung fehlgeschlagen", "Falha na geração da senha"],
    ["Password output failed", "Falló la salida de la contraseña", "Échec de l’affichage du mot de passe", "Passwortausgabe fehlgeschlagen", "Falha na exibição da senha"],
    ["not installed", "no instalado", "non installé", "nicht installiert", "não instalado"],
    ["Stopped", "Detenido", "Arrêté", "Beendet", "Parado"],
    ["Running", "En ejecución", "En cours", "Wird ausgeführt", "Em execução"],
    ["StartPending", "Inicio pendiente", "Démarrage en attente", "Start ausstehend", "Início pendente"],
    ["StopPending", "Parada pendiente", "Arrêt en attente", "Beenden ausstehend", "Parada pendente"],
    ["ContinuePending", "Reanudación pendiente", "Reprise en attente", "Fortsetzen ausstehend", "Continuação pendente"],
    ["PausePending", "Pausa pendiente", "Pause en attente", "Pause ausstehend", "Pausa pendente"],
    ["Paused", "En pausa", "En pause", "Angehalten", "Pausado"],
    ["exit=", "salida=", "sortie=", "Exitcode=", "saída="],
    ["checkpoint=", "punto de control=", "point de contrôle=", "Prüfpunkt=", "ponto de controle="],
    ["wait=", "espera=", "attente=", "Wartezeit=", "espera="],
    ["App Installer package path is not absolute", "La ruta del paquete App Installer no es absoluta", "Le chemin du paquet App Installer n’est pas absolu", "App-Installer-Paketpfad ist nicht absolut", "O caminho do pacote App Installer não é absoluto"],
    ["Resolve App Installer package directory", "Resolver el directorio del paquete App Installer", "Résoudre le dossier du paquet App Installer", "App-Installer-Paketverzeichnis auflösen", "Resolver o diretório do pacote App Installer"],
    ["Resolve packaged winget.exe", "Resolver winget.exe del paquete", "Résoudre winget.exe du paquet", "Paketdatei winget.exe auflösen", "Resolver winget.exe do pacote"],
    ["Packaged winget.exe escapes its registered package directory or is not a file", "winget.exe está fuera del directorio registrado del paquete o no es un archivo", "winget.exe est hors du dossier enregistré du paquet ou n’est pas un fichier", "winget.exe liegt außerhalb des registrierten Paketverzeichnisses oder ist keine Datei", "winget.exe está fora do diretório registrado do pacote ou não é um arquivo"],
    ["Invalid TokenUser buffer size", "Tamaño de búfer TokenUser no válido", "Taille du tampon TokenUser non valide", "Ungültige TokenUser-Puffergröße", "Tamanho de buffer TokenUser inválido"],
    ["Open desktop shell process", "Abrir el proceso del escritorio", "Ouvrir le processus du bureau", "Desktop-Shell-Prozess öffnen", "Abrir o processo do desktop"],
    ["SHGetKnownFolderPath failed", "Falló SHGetKnownFolderPath", "Échec de SHGetKnownFolderPath", "SHGetKnownFolderPath fehlgeschlagen", "Falha em SHGetKnownFolderPath"],
    ["SHGetKnownFolderPath returned a null path", "SHGetKnownFolderPath devolvió una ruta nula", "SHGetKnownFolderPath a renvoyé un chemin nul", "SHGetKnownFolderPath lieferte einen Nullpfad", "SHGetKnownFolderPath retornou um caminho nulo"],
    ["Known folder is not an absolute path", "La carpeta conocida no tiene una ruta absoluta", "Le dossier connu n’a pas de chemin absolu", "Bekannter Ordner hat keinen absoluten Pfad", "A pasta conhecida não tem um caminho absoluto"],
    ["Check existing Bitwarden", "Comprobar Bitwarden existente", "Vérifier Bitwarden existant", "Vorhandenes Bitwarden prüfen", "Verificar Bitwarden existente"],
    ["OpenPackageInfoByFullName failed", "Falló OpenPackageInfoByFullName", "Échec de OpenPackageInfoByFullName", "OpenPackageInfoByFullName fehlgeschlagen", "Falha em OpenPackageInfoByFullName"],
    ["GetPackageInfo sizing failed", "Falló el cálculo de tamaño de GetPackageInfo", "Échec du calcul de taille GetPackageInfo", "Größenabfrage GetPackageInfo fehlgeschlagen", "Falha no cálculo de tamanho de GetPackageInfo"],
    ["GetPackageInfo failed", "Falló GetPackageInfo", "Échec de GetPackageInfo", "GetPackageInfo fehlgeschlagen", "Falha em GetPackageInfo"],
    ["App Installer is a developer-mode registration; use the packaged Microsoft Store installation", "App Installer está registrado en modo de desarrollador; use la instalación empaquetada de Microsoft Store", "App Installer est inscrit en mode développeur ; utilisez le paquet Microsoft Store", "App Installer ist im Entwicklermodus registriert; Paketinstallation aus Microsoft Store verwenden", "App Installer está registrado no modo de desenvolvedor; use o pacote da Microsoft Store"],
    ["Microsoft App Installer is not available for this user", "Microsoft App Installer no está disponible para este usuario", "Microsoft App Installer n’est pas disponible pour cet utilisateur", "Microsoft App Installer ist für diesen Benutzer nicht verfügbar", "Microsoft App Installer não está disponível para este usuário"],
    ["install or repair App Installer through Microsoft Store", "instale o repare App Installer mediante Microsoft Store", "installez ou réparez App Installer via Microsoft Store", "App Installer über Microsoft Store installieren oder reparieren", "instale ou repare App Installer pela Microsoft Store"],
    ["GetPackagesByPackageFamily failed", "Falló GetPackagesByPackageFamily", "Échec de GetPackagesByPackageFamily", "GetPackagesByPackageFamily fehlgeschlagen", "Falha em GetPackagesByPackageFamily"],
    ["GetPackagePathByFullName sizing failed", "Falló el cálculo de tamaño de GetPackagePathByFullName", "Échec du calcul de taille GetPackagePathByFullName", "Größenabfrage GetPackagePathByFullName fehlgeschlagen", "Falha no cálculo de tamanho de GetPackagePathByFullName"],
    ["GetPackagePathByFullName failed", "Falló GetPackagePathByFullName", "Échec de GetPackagePathByFullName", "GetPackagePathByFullName fehlgeschlagen", "Falha em GetPackagePathByFullName"],
    ["Unterminated package path", "Ruta de paquete sin terminador", "Chemin de paquet non terminé", "Paketpfad ohne Abschlusszeichen", "Caminho de pacote sem terminador"],
    ["Check registered App Installer executable", "Comprobar el ejecutable registrado de App Installer", "Vérifier l’exécutable enregistré d’App Installer", "Registrierte App-Installer-Programmdatei prüfen", "Verificar o executável registrado do App Installer"],
    ["Expected one registered App Installer executable, found", "Se esperaba un ejecutable registrado de App Installer; encontrados", "Un exécutable enregistré d’App Installer attendu ; trouvés", "Eine registrierte App-Installer-Programmdatei erwartet; gefunden", "Era esperado um executável registrado do App Installer; encontrados"],
    ["repair App Installer for this user", "repare App Installer para este usuario", "réparez App Installer pour cet utilisateur", "App Installer für diesen Benutzer reparieren", "repare App Installer para este usuário"],
    ["Invalid packaged executable path", "Ruta de ejecutable empaquetado no válida", "Chemin de l’exécutable du paquet non valide", "Ungültiger Pfad der Paketprogrammdatei", "Caminho do executável do pacote inválido"],
    ["Missing package directory", "Falta el directorio del paquete", "Dossier du paquet manquant", "Paketverzeichnis fehlt", "Diretório do pacote ausente"],
    ["Protect pipe reader from inheritance", "Proteger el lector de canal contra herencia", "Protéger le lecteur du canal contre l’héritage", "Pipe-Lesehandle vor Vererbung schützen", "Proteger o leitor do canal contra herança"],
    ["Open null input", "Abrir entrada nula", "Ouvrir l’entrée nulle", "Nulleingabe öffnen", "Abrir entrada nula"],
    ["Set null input inheritance", "Configurar la herencia de entrada nula", "Configurer l’héritage de l’entrée nulle", "Vererbung der Nulleingabe festlegen", "Configurar herança de entrada nula"],
    ["Start packaged WinGet", "Iniciar WinGet empaquetado", "Démarrer WinGet du paquet", "WinGet aus dem Paket starten", "Iniciar WinGet do pacote"],
    ["Assign WinGet to timeout job", "Asignar WinGet al trabajo con límite de tiempo", "Associer WinGet à la tâche avec délai", "WinGet dem zeitbegrenzten Auftrag zuordnen", "Associar WinGet à tarefa com limite de tempo"],
    ["Resume WinGet failed", "Falló la reanudación de WinGet", "Échec de reprise de WinGet", "Fortsetzen von WinGet fehlgeschlagen", "Falha ao retomar WinGet"],
    ["exceeded the ten-minute deadline; its job was terminated; check installation state before retrying", "superó el límite de diez minutos; su trabajo se terminó; compruebe la instalación antes de reintentar", "a dépassé le délai de dix minutes ; sa tâche a été arrêtée ; vérifiez l’installation avant de réessayer", "überschritt das Zeitlimit von zehn Minuten; Auftrag beendet; Installationszustand vor erneutem Versuch prüfen", "excedeu o limite de dez minutos; a tarefa foi encerrada; verifique a instalação antes de tentar novamente"],
    ["Read WinGet output pipe", "Leer el canal de salida de WinGet", "Lire le canal de sortie WinGet", "WinGet-Ausgabepipe lesen", "Ler o canal de saída do WinGet"],
    ["Read WinGet output", "Leer la salida de WinGet", "Lire la sortie WinGet", "WinGet-Ausgabe lesen", "Ler a saída do WinGet"],
    ["Source", "Origen", "Source", "Quelle", "Origem"],
    ["List", "Lista", "Liste", "Liste", "Lista"],
    ["Install", "Instalación", "Installation", "Installation", "Instalação"],
    ["exit", "salida", "sortie", "Exitcode", "saída"],
    ["count", "cantidad", "nombre", "Anzahl", "quantidade"],
    ["Run tools bitwarden --yes from the original user's non-elevated desktop, not an administrator terminal", "Ejecute tools bitwarden --yes desde el escritorio del usuario original sin elevar, no desde una terminal de administrador", "Exécutez tools bitwarden --yes depuis le bureau non élevé de l’utilisateur initial, pas depuis un terminal administrateur", "tools bitwarden --yes vom Desktop des ursprünglichen Benutzers ohne Rechteerhöhung ausführen, nicht im Administratorterminal", "Execute tools bitwarden --yes no desktop do usuário original sem elevação, não em um terminal de administrador"],
    ["No desktop shell: Bitwarden installation cannot run as a service or background account", "No hay shell de escritorio: Bitwarden no se puede instalar como servicio o cuenta en segundo plano", "Aucun shell de bureau : Bitwarden ne peut pas être installé depuis un service ou un compte en arrière-plan", "Keine Desktop-Shell: Bitwarden kann nicht als Dienst oder Hintergrundkonto installiert werden", "Sem shell de desktop: o Bitwarden não pode ser instalado como serviço ou conta em segundo plano"],
    ["Cannot identify desktop shell user", "No se puede identificar al usuario del escritorio", "Impossible d’identifier l’utilisateur du bureau", "Desktopbenutzer nicht feststellbar", "Não foi possível identificar o usuário do desktop"],
    ["Current account differs from the desktop user; run tools bitwarden --yes as that user without elevation", "La cuenta actual difiere del usuario del escritorio; ejecute tools bitwarden --yes como ese usuario sin elevación", "Le compte actuel diffère de l’utilisateur du bureau ; exécutez tools bitwarden --yes sous ce compte sans élévation", "Aktuelles Konto unterscheidet sich vom Desktopbenutzer; tools bitwarden --yes als dieser Benutzer ohne Rechteerhöhung ausführen", "A conta atual difere do usuário do desktop; execute tools bitwarden --yes como esse usuário sem elevação"],
    ["WinGet Bitwarden detection failed", "Falló la detección de Bitwarden con WinGet", "Échec de la détection de Bitwarden par WinGet", "Bitwarden-Erkennung durch WinGet fehlgeschlagen", "Falha na detecção do Bitwarden pelo WinGet"],
    ["WinGet source export did not return a single JSON source", "La exportación de WinGet no devolvió un único origen JSON", "L’export WinGet n’a pas renvoyé une source JSON unique", "WinGet-Quellexport lieferte keine einzelne JSON-Quelle", "A exportação do WinGet não retornou uma única origem JSON"],
    ["WinGet repository verification failed: unexpected", "Falló la verificación del repositorio WinGet: valor inesperado", "Échec de vérification du dépôt WinGet : valeur inattendue", "WinGet-Repositoryprüfung fehlgeschlagen: unerwartet", "Falha na verificação do repositório WinGet: valor inesperado"],
    ["Bitwarden operation exceeded its ten-minute deadline", "La operación de Bitwarden superó el límite de diez minutos", "L’opération Bitwarden a dépassé le délai de dix minutes", "Bitwarden-Vorgang überschritt das Zeitlimit von zehn Minuten", "A operação do Bitwarden excedeu o limite de dez minutos"],
    ["WinGet source export failed", "Falló la exportación del origen WinGet", "Échec de l’export de la source WinGet", "WinGet-Quellexport fehlgeschlagen", "Falha na exportação da origem WinGet"],
    ["Determine existing installation; no install was attempted", "Comprobar instalación existente; no se intentó instalar", "Vérification de l’installation existante ; aucune installation tentée", "Vorhandene Installation prüfen; kein Installationsversuch erfolgt", "Verificar instalação existente; nenhuma instalação foi tentada"],
    ["WinGet Bitwarden installation failed", "Falló la instalación de Bitwarden con WinGet", "Échec de l’installation de Bitwarden par WinGet", "Bitwarden-Installation durch WinGet fehlgeschlagen", "Falha na instalação do Bitwarden pelo WinGet"],
    ["installer hash verification was not bypassed", "no se omitió la verificación del hash del instalador", "la vérification du hachage de l’installeur n’a pas été contournée", "Installer-Hashprüfung wurde nicht umgangen", "a verificação do hash do instalador não foi ignorada"],
    ["WinGet reported success but Bitwarden desktop was not found afterward", "WinGet informó éxito, pero no se encontró Bitwarden después", "WinGet a signalé un succès, mais Bitwarden n’a pas été trouvé ensuite", "WinGet meldete Erfolg, aber Bitwarden wurde anschließend nicht gefunden", "O WinGet informou sucesso, mas o Bitwarden não foi encontrado depois"],
    ["Invalid elevation arguments", "Argumentos de elevación no válidos", "Arguments d’élévation non valides", "Ungültige Argumente zur Rechteerhöhung", "Argumentos de elevação inválidos"],
    ["Elevation returned no process handle", "La elevación no devolvió un identificador de proceso", "L’élévation n’a renvoyé aucun handle de processus", "Rechteerhöhung lieferte kein Prozesshandle", "A elevação não retornou um identificador de processo"],
    ["--yes authorizes downloading and installing Bitwarden from the Microsoft WinGet repository and accepting Bitwarden package licenses/agreements and WinGet source agreements. Another password manager is a valid choice.", "--yes autoriza descargar e instalar Bitwarden desde el repositorio Microsoft WinGet y aceptar las licencias/acuerdos del paquete Bitwarden y los acuerdos de origen de WinGet. Puede elegir otro gestor de contraseñas.", "--yes autorise le téléchargement et l’installation de Bitwarden depuis le dépôt Microsoft WinGet ainsi que l’acceptation des licences/accords du paquet Bitwarden et des accords de source WinGet. Vous pouvez choisir un autre gestionnaire de mots de passe.", "--yes erlaubt Download und Installation von Bitwarden aus dem Microsoft-WinGet-Repository sowie die Annahme der Bitwarden-Paketlizenzen/-vereinbarungen und WinGet-Quellvereinbarungen. Ein anderer Passwortmanager ist ebenfalls eine gültige Wahl.", "--yes autoriza baixar e instalar o Bitwarden do repositório Microsoft WinGet e aceitar as licenças/acordos do pacote Bitwarden e os acordos de origem do WinGet. Você pode escolher outro gerenciador de senhas."],
    ["Bitwarden installation is supported only on Windows", "La instalación de Bitwarden solo es compatible con Windows", "L’installation de Bitwarden est disponible uniquement sous Windows", "Die Bitwarden-Installation wird nur unter Windows unterstützt", "A instalação do Bitwarden só é compatível com Windows"],
    ["Defender preference is not a readable boolean", "La preferencia de Defender no es un booleano legible", "La préférence Defender n’est pas un booléen lisible", "Defender-Einstellung ist kein lesbarer boolescher Wert", "A preferência do Defender não é um booleano legível"],
    ["Firewall enabled preference is not a concrete boolean", "La preferencia de activación del cortafuegos no es un booleano concreto", "La préférence d’activation du pare-feu n’est pas un booléen défini", "Firewall-Aktivierungseinstellung ist kein eindeutiger boolescher Wert", "A preferência de ativação do firewall não é um booleano concreto"],
    ["UAC value is not a DWORD", "El valor UAC no es DWORD", "La valeur UAC n’est pas un DWORD", "UAC-Wert ist kein DWORD", "O valor UAC não é DWORD"],
    ["UAC repair requires an explicitly disabled current setting", "La reparación UAC requiere una configuración actual explícitamente desactivada", "La réparation UAC nécessite un paramètre actuel explicitement désactivé", "UAC-Reparatur erfordert eine ausdrücklich deaktivierte aktuelle Einstellung", "A reparação UAC exige uma configuração atual explicitamente desativada"],
    ["Cannot read all effective firewall profiles", "No se pueden leer todos los perfiles efectivos del cortafuegos", "Impossible de lire tous les profils effectifs du pare-feu", "Nicht alle wirksamen Firewallprofile sind lesbar", "Não é possível ler todos os perfis efetivos do firewall"],
    ["No readable volume status", "No hay estado de volumen legible", "Aucun état de volume lisible", "Kein lesbarer Volumestatus", "Nenhum estado de volume legível"],
    ["Offline update query did not fully succeed", "La consulta de actualizaciones sin conexión no se completó correctamente", "La recherche de mises à jour hors ligne n’a pas entièrement réussi", "Offline-Updateabfrage nicht vollständig erfolgreich", "A consulta de atualizações offline não foi totalmente bem-sucedida"],
    ["Secblitz requires Windows 10/11 x64; this platform cannot assess or change Windows", "Secblitz requiere Windows 10/11 x64; esta plataforma no puede evaluar ni modificar Windows", "Secblitz nécessite Windows 10/11 x64 ; cette plateforme ne peut ni évaluer ni modifier Windows", "Secblitz benötigt Windows 10/11 x64; diese Plattform kann Windows weder bewerten noch ändern", "Secblitz exige Windows 10/11 x64; esta plataforma não pode avaliar nem alterar o Windows"],
    ["Elevation is only supported on Windows", "La elevación solo es compatible con Windows", "L’élévation des privilèges est disponible uniquement sous Windows", "Rechteerhöhung wird nur unter Windows unterstützt", "A elevação só é compatível com Windows"],
    ["Windows services are only supported on Windows", "Los servicios de Windows solo son compatibles con Windows", "Les services Windows sont disponibles uniquement sous Windows", "Windows-Dienste werden nur unter Windows unterstützt", "Serviços do Windows só são compatíveis com Windows"],
    ["Run JSON reports from an administrator terminal.", "Ejecute los informes JSON desde una terminal de administrador.", "Exécutez les rapports JSON depuis un terminal administrateur.", "JSON-Berichte in einem Administratorterminal ausführen.", "Execute relatórios JSON em um terminal de administrador."],
    ["Registered antivirus: ", "Antivirus registrado: ", "Antivirus enregistré : ", "Registriertes Antivirenprogramm: ", "Antivírus registrado: "],
    ["; registered firewall: ", "; cortafuegos registrado: ", "; pare-feu enregistré : ", "; registrierte Firewall: ", "; firewall registrado: "],
    ["Registration alone does not establish provider health. Additional or unrecognized registrations block the corresponding Defender/firewall changes, even when reported inactive.", "El registro por sí solo no demuestra el buen estado del proveedor. Los registros adicionales o desconocidos bloquean los cambios correspondientes de Defender/cortafuegos, incluso si figuran como inactivos.", "L’enregistrement seul ne garantit pas le bon fonctionnement. Les inscriptions supplémentaires ou inconnues bloquent les modifications correspondantes de Defender/pare-feu, même signalées inactives.", "Die Registrierung allein belegt keinen funktionierenden Schutz. Zusätzliche oder unbekannte Registrierungen verhindern entsprechende Defender-/Firewall-Änderungen, auch wenn sie als inaktiv gemeldet werden.", "O registro por si só não comprova o funcionamento do provedor. Registros adicionais ou desconhecidos bloqueiam as alterações correspondentes do Defender/firewall, mesmo quando indicados como inativos."],
    ["ActiveStore values are shown; NotConfigured does not establish the effective default action. Rules and outbound preferences are preserved.", "Se muestran valores de ActiveStore; NotConfigured no demuestra la acción predeterminada efectiva. Se conservan las reglas y preferencias de salida.", "Les valeurs ActiveStore sont affichées ; NotConfigured ne prouve pas l’action par défaut effective. Les règles et préférences sortantes sont conservées.", "ActiveStore-Werte werden angezeigt; NotConfigured belegt keine wirksame Standardaktion. Regeln und ausgehende Einstellungen bleiben erhalten.", "Valores de ActiveStore são exibidos; NotConfigured não comprova a ação padrão efetiva. Regras e preferências de saída são preservadas."],
    ["Check Windows Security for effective protection and signature updates.", "Consulte Seguridad de Windows para comprobar la protección efectiva y actualizar firmas.", "Consultez Sécurité Windows pour vérifier la protection effective et les mises à jour des signatures.", "Wirksamen Schutz und Signaturupdates in Windows-Sicherheit prüfen.", "Consulte a Segurança do Windows para verificar a proteção efetiva e atualizar assinaturas."],
    ["; hidden exclusions cannot be ruled out. Exclusions are preserved.", "; no se pueden descartar exclusiones ocultas. Se conservan las exclusiones.", "; des exclusions masquées restent possibles. Les exclusions sont conservées.", "; versteckte Ausschlüsse sind nicht auszuschließen. Ausschlüsse bleiben erhalten.", "; exclusões ocultas não podem ser descartadas. As exclusões são preservadas."],
    ["Standard Windows 10 support ended October 14, 2025. ESU enrollment and LTSC/IoT editions have different support terms; enrollment/support entitlement is not verified. Windows 11 support depends on release and edition; check Microsoft's lifecycle information.", "El soporte estándar de Windows 10 terminó el 14 de octubre de 2025. La inscripción ESU y las ediciones LTSC/IoT tienen condiciones distintas; no se verifica la inscripción ni el derecho a soporte. El soporte de Windows 11 depende de la versión y edición; consulte el ciclo de vida de Microsoft.", "Le support standard de Windows 10 a pris fin le 14 octobre 2025. L’inscription ESU et les éditions LTSC/IoT ont des conditions différentes ; les droits au support ne sont pas vérifiés. Le support de Windows 11 dépend de la version et de l’édition ; consultez le cycle de vie Microsoft.", "Der reguläre Windows-10-Support endete am 14. Oktober 2025. ESU-Teilnahme und LTSC-/IoT-Editionen haben andere Bedingungen; Teilnahme und Supportanspruch werden nicht geprüft. Windows-11-Support hängt von Version und Edition ab; Microsoft-Lebenszyklusinformationen prüfen.", "O suporte padrão do Windows 10 terminou em 14 de outubro de 2025. A inscrição ESU e as edições LTSC/IoT têm condições diferentes; a inscrição e o direito ao suporte não são verificados. O suporte do Windows 11 depende da versão e edição; consulte o ciclo de vida da Microsoft."],
    ["Recovery-key backup is not verified.", "No se verifica la copia de seguridad de la clave de recuperación.", "La sauvegarde de la clé de récupération n’est pas vérifiée.", "Die Sicherung des Wiederherstellungsschlüssels wird nicht geprüft.", "O backup da chave de recuperação não é verificado."],
    ["Unsupported firmware or inaccessible status is reported as unknown.", "El firmware no compatible o el estado inaccesible se indica como desconocido.", "Un micrologiciel incompatible ou un état inaccessible est indiqué comme inconnu.", "Nicht unterstützte Firmware oder unzugänglicher Status wird als unbekannt gemeldet.", "Firmware incompatível ou estado inacessível é indicado como desconhecido."],
    ["Locally cached pending updates=", "Actualizaciones pendientes en caché local=", "Mises à jour en attente dans le cache local=", "Lokal zwischengespeicherte ausstehende Updates=", "Atualizações pendentes em cache local="],
    ["This offline result does not establish current patch compliance; open Windows Update and check for updates.", "Este resultado sin conexión no demuestra que los parches estén al día; abra Windows Update y busque actualizaciones.", "Ce résultat hors ligne ne prouve pas que les correctifs sont à jour ; ouvrez Windows Update et recherchez les mises à jour.", "Dieses Offline-Ergebnis belegt keinen aktuellen Patchstand; Windows Update öffnen und nach Updates suchen.", "Este resultado offline não comprova que as correções estejam em dia; abra o Windows Update e procure atualizações."],
    ["Deny incoming Remote Desktop connections=", "Denegar conexiones entrantes de Escritorio remoto=", "Refuser les connexions entrantes du Bureau à distance=", "Eingehende Remotedesktopverbindungen verweigern=", "Negar conexões de entrada da Área de Trabalho Remota="],
    ["Review need, network exposure and Network Level Authentication; no changes made.", "Revise la necesidad, exposición de red y autenticación a nivel de red; no se realizaron cambios.", "Vérifiez le besoin, l’exposition réseau et l’authentification au niveau du réseau ; aucune modification effectuée.", "Bedarf, Netzwerkexposition und Authentifizierung auf Netzwerkebene prüfen; keine Änderungen vorgenommen.", "Revise a necessidade, exposição de rede e autenticação em nível de rede; nenhuma alteração realizada."],
    ["SMB1 optional feature state=", "Estado de la característica opcional SMB1=", "État de la fonctionnalité facultative SMB1=", "Status der optionalen SMB1-Funktion=", "Estado do recurso opcional SMB1="],
    ["Review dependencies before removing legacy protocol support.", "Revise las dependencias antes de eliminar el soporte de protocolos antiguos.", "Vérifiez les dépendances avant de supprimer la prise en charge des anciens protocoles.", "Abhängigkeiten vor dem Entfernen veralteter Protokollunterstützung prüfen.", "Revise as dependências antes de remover o suporte a protocolos antigos."],
    ["Enabled local accounts whose PasswordRequired flag is false: ", "Cuentas locales habilitadas cuyo indicador PasswordRequired es falso: ", "Comptes locaux activés dont l’indicateur PasswordRequired est faux : ", "Aktivierte lokale Konten mit falschem PasswordRequired-Flag: ", "Contas locais habilitadas cujo indicador PasswordRequired é falso: "],
    ["This flag does not reveal password presence, strength, reuse or Windows Hello security. Review account access and use strong unique passwords/MFA where supported.", "Este indicador no revela la presencia, fortaleza o reutilización de contraseñas ni la seguridad de Windows Hello. Revise el acceso a cuentas y use contraseñas fuertes y únicas/MFA donde sea compatible.", "Cet indicateur ne révèle ni présence, ni robustesse, ni réutilisation des mots de passe, ni sécurité de Windows Hello. Vérifiez l’accès aux comptes et utilisez des mots de passe uniques et robustes/MFA si disponible.", "Dieses Flag zeigt weder Vorhandensein, Stärke oder Wiederverwendung von Passwörtern noch die Sicherheit von Windows Hello. Kontozugriff prüfen und starke einmalige Passwörter/MFA verwenden, soweit unterstützt.", "Este indicador não revela presença, força ou reutilização de senhas nem a segurança do Windows Hello. Revise o acesso às contas e use senhas fortes e únicas/MFA quando compatível."],
    ["Review Core isolation in Windows Security and driver compatibility before enabling.", "Revise Aislamiento del núcleo en Seguridad de Windows y la compatibilidad de controladores antes de activarlo.", "Vérifiez l’isolation du noyau dans Sécurité Windows et la compatibilité des pilotes avant l’activation.", "Vor Aktivierung Kernisolierung in Windows-Sicherheit und Treiberkompatibilität prüfen.", "Revise o Isolamento do núcleo na Segurança do Windows e a compatibilidade dos drivers antes de ativar."],
    ["Mode=", "Modo=", "Mode=", "Modus=", "Modo="],
    ["service=", "servicio=", "service=", "Dienst=", "serviço="],
    ["antivirus=", "antivirus=", "antivirus=", "Antivirus=", "antivírus="],
    ["realtime=", "tiempo real=", "temps réel=", "Echtzeit=", "tempo real="],
    ["behavior=", "comportamiento=", "comportement=", "Verhalten=", "comportamento="],
    ["archive preference disabled=", "preferencia de archivos comprimidos desactivada=", "préférence d’analyse des archives désactivée=", "Archivprüfung deaktiviert=", "preferência de arquivos compactados desativada="],
    ["tamper protected=", "protección contra alteraciones=", "protection contre les falsifications=", "Manipulationsschutz=", "proteção contra adulterações="],
    ["signatures=", "firmas=", "signatures=", "Signaturen=", "assinaturas="],
    ["updated=", "actualizadas=", "mises à jour=", "aktualisiert=", "atualizadas="],
    ["age days=", "antigüedad en días=", "ancienneté en jours=", "Alter in Tagen=", "idade em dias="],
    ["Exclusion counts: paths=", "Cantidad de exclusiones: rutas=", "Nombre d’exclusions : chemins=", "Anzahl der Ausschlüsse: Pfade=", "Quantidade de exclusões: caminhos="],
    ["processes=", "procesos=", "processus=", "Prozesse=", "processos="],
    ["extensions=", "extensiones=", "extensions=", "Erweiterungen=", "extensões="],
    ["enabled=", "habilitado=", "activé=", "aktiviert=", "habilitado="],
    ["inbound=", "entrada=", "entrant=", "eingehend=", "entrada="],
    ["outbound=", "salida=", "sortant=", "ausgehend=", "saída="],
    ["protection=", "protección=", "protection=", "Schutz=", "proteção="],
    ["state=", "estado=", "état=", "Status=", "estado="],
    ["Secure Boot enabled=", "Arranque seguro habilitado=", "Démarrage sécurisé activé=", "Sicherer Start aktiviert=", "Inicialização segura habilitada="],
    ["HVCI running=", "HVCI en ejecución=", "HVCI en cours=", "HVCI läuft=", "HVCI em execução="],
    ["configured=", "configurado=", "configuré=", "konfiguriert=", "configurado="],
    ["OS=", "SO=", "OS=", "Betriebssystem=", "SO="],
    ["version=", "versión=", "version=", "Version=", "versão="],
    ["build=", "compilación=", "build=", "Build=", "compilação="],
    ["Unsupported Windows client capability", "Capacidad de cliente Windows no compatible", "Fonctionnalité du client Windows non prise en charge", "Nicht unterstützte Windows-Clientfunktion", "Capacidade do cliente Windows incompatível"],
    ["Domain membership is not readable", "No se puede leer la pertenencia al dominio", "L’appartenance au domaine est illisible", "Domänenmitgliedschaft nicht lesbar", "Não é possível ler a associação ao domínio"],
    ["Domain-managed machine: assessment only", "Equipo administrado por dominio: solo evaluación", "Machine gérée par domaine : évaluation uniquement", "Domänenverwalteter Computer: nur Bewertung", "Máquina gerenciada por domínio: apenas avaliação"],
    ["Enrollment or cloud-management evidence: assessment only", "Indicios de inscripción o administración en la nube: solo evaluación", "Indices d’inscription ou de gestion cloud : évaluation uniquement", "Registrierungs- oder Cloudverwaltungshinweise: nur Bewertung", "Indícios de inscrição ou gerenciamento em nuvem: apenas avaliação"],
    ["Configured management/security policy: assessment only", "Directiva de administración/seguridad configurada: solo evaluación", "Stratégie de gestion/sécurité configurée : évaluation uniquement", "Verwaltungs-/Sicherheitsrichtlinie konfiguriert: nur Bewertung", "Política de gerenciamento/segurança configurada: apenas avaliação"],
    ["Applied computer Group Policy: assessment only", "Directiva de grupo aplicada al equipo: solo evaluación", "Stratégie de groupe appliquée : évaluation uniquement", "Angewendete Computergruppenrichtlinie: nur Bewertung", "Política de grupo aplicada ao computador: apenas avaliação"],
    ["Local computer policy artifacts: assessment only", "Indicios de directiva local del equipo: solo evaluación", "Traces de stratégie locale : évaluation uniquement", "Lokale Computerrichtlinien vorhanden: nur Bewertung", "Indícios de política local do computador: apenas avaliação"],
    ["Additional or unrecognized security provider: assessment only", "Proveedor de seguridad adicional o desconocido: solo evaluación", "Fournisseur de sécurité supplémentaire ou inconnu : évaluation uniquement", "Zusätzlicher oder unbekannter Sicherheitsanbieter: nur Bewertung", "Provedor de segurança adicional ou desconhecido: apenas avaliação"],
    ["Defender provider registration cannot be established", "No se puede confirmar el registro del proveedor Defender", "Impossible de confirmer l’enregistrement de Defender", "Defender-Anbieterregistrierung nicht feststellbar", "Não é possível confirmar o registro do provedor Defender"],
    ["Defender tamper-protection state is unknown", "Estado de protección contra alteraciones de Defender desconocido", "État de protection contre les falsifications de Defender inconnu", "Defender-Manipulationsschutzstatus unbekannt", "Estado da proteção contra adulterações do Defender desconhecido"],
    ["Defender unavailable, passive, or tamper protected: assessment only", "Defender no disponible, pasivo o protegido contra alteraciones: solo evaluación", "Defender indisponible, passif ou protégé contre les falsifications : évaluation uniquement", "Defender nicht verfügbar, passiv oder manipulationsgeschützt: nur Bewertung", "Defender indisponível, passivo ou protegido contra adulterações: apenas avaliação"],
    ["Firewall services unavailable", "Servicios de cortafuegos no disponibles", "Services du pare-feu indisponibles", "Firewalldienste nicht verfügbar", "Serviços de firewall indisponíveis"],
    ["Firewall profile has resultant Group Policy: assessment only", "El perfil de cortafuegos tiene directiva de grupo resultante: solo evaluación", "Le profil du pare-feu a une stratégie de groupe résultante : évaluation uniquement", "Firewallprofil hat eine resultierende Gruppenrichtlinie: nur Bewertung", "O perfil de firewall tem política de grupo resultante: apenas avaliação"],
    ["Firewall profile capability cannot be established", "No se puede determinar la capacidad del perfil de cortafuegos", "Impossible de déterminer les capacités du profil de pare-feu", "Funktionen des Firewallprofils nicht feststellbar", "Não é possível determinar a capacidade do perfil de firewall"],
    ["Conservative, reversible Windows hardening", "Protección prudente y reversible de Windows", "Sécurisation prudente et réversible de Windows", "Vorsichtige, umkehrbare Windows-Härtung", "Proteção prudente e reversível do Windows"],
    ["Audit security preferences", "Auditar preferencias de seguridad", "Auditer les préférences de sécurité", "Sicherheitseinstellungen prüfen", "Auditar preferências de segurança"],
    ["Apply conservative protection", "Aplicar protección prudente", "Appliquer une protection prudente", "Vorsichtige Schutzmaßnahmen anwenden", "Aplicar proteção prudente"],
    ["Restore the latest recorded transaction", "Restaurar la última transacción registrada", "Restaurer la dernière transaction enregistrée", "Letzte protokollierte Transaktion zurücksetzen", "Restaurar a última transação registrada"],
    ["Show transaction history", "Mostrar historial de transacciones", "Afficher l’historique des transactions", "Transaktionsverlauf anzeigen", "Mostrar histórico de transações"],
    ["Generate a 24-character password on this terminal only", "Generar una contraseña de 24 caracteres solo en esta terminal", "Générer un mot de passe de 24 caractères uniquement dans ce terminal", "Ein Passwort mit 24 Zeichen nur in diesem Terminal erzeugen", "Gerar uma senha de 24 caracteres apenas neste terminal"],
    ["Manage the optional service", "Administrar el servicio opcional", "Gérer le service facultatif", "Optionalen Dienst verwalten", "Gerenciar o serviço opcional"],
    ["Optional software tools", "Herramientas de software opcionales", "Outils logiciels facultatifs", "Optionale Softwarewerkzeuge", "Ferramentas de software opcionais"],
    ["Install the service", "Instalar el servicio", "Installer le service", "Dienst installieren", "Instalar o serviço"],
    ["Uninstall the service", "Desinstalar el servicio", "Désinstaller le service", "Dienst deinstallieren", "Desinstalar o serviço"],
    ["Query service status", "Consultar estado del servicio", "Consulter l’état du service", "Dienststatus abfragen", "Consultar estado do serviço"],
    ["Run the service dispatcher", "Ejecutar el despachador del servicio", "Exécuter le répartiteur du service", "Dienststeuerung ausführen", "Executar o despachante do serviço"],
    ["Install Bitwarden with explicit consent", "Instalar Bitwarden con consentimiento explícito", "Installer Bitwarden avec consentement explicite", "Bitwarden mit ausdrücklicher Zustimmung installieren", "Instalar Bitwarden com consentimento explícito"],
    ["Consent to downloading and installing Bitwarden", "Aceptar la descarga e instalación de Bitwarden", "Accepter le téléchargement et l’installation de Bitwarden", "Download und Installation von Bitwarden zustimmen", "Autorizar o download e a instalação do Bitwarden"],
    ["Language (default: Windows display language)", "Idioma (predeterminado: idioma de Windows)", "Langue (par défaut : langue de Windows)", "Sprache (Standard: Windows-Anzeigesprache)", "Idioma (padrão: idioma do Windows)"],
    ["Disable terminal animation", "Desactivar animación de terminal", "Désactiver l’animation du terminal", "Terminalanimation deaktivieren", "Desativar animação do terminal"],
    ["Output raw JSON reports only (audit/apply/revert/history)", "Emitir solo informes JSON (audit/apply/revert/history)", "Émettre uniquement les rapports JSON (audit/apply/revert/history)", "Nur JSON-Berichte ausgeben (audit/apply/revert/history)", "Emitir apenas relatórios JSON (audit/apply/revert/history)"],
    ["Show help", "Mostrar ayuda", "Afficher l’aide", "Hilfe anzeigen", "Mostrar ajuda"],
    ["Show version", "Mostrar versión", "Afficher la version", "Version anzeigen", "Mostrar versão"],
    ["Usage", "Uso", "Utilisation", "Aufruf", "Uso"],
    ["Commands", "Comandos", "Commandes", "Befehle", "Comandos"],
    ["Options", "Opciones", "Options", "Optionen", "Opções"],
    ["No command: request administrator access and apply conservative protection automatically.", "Sin comando: solicitar acceso de administrador y aplicar protección prudente automáticamente.", "Sans commande : demander les droits administrateur et appliquer automatiquement une protection prudente.", "Ohne Befehl: Administratorrechte anfordern und vorsichtige Schutzmaßnahmen automatisch anwenden.", "Sem comando: solicitar acesso de administrador e aplicar proteção prudente automaticamente."],
    ["Working", "Procesando", "Traitement en cours", "In Bearbeitung", "Processando"],
    ["Complete", "Completado", "Terminé", "Abgeschlossen", "Concluído"],
    ["Review needed", "Revisión necesaria", "Vérification nécessaire", "Überprüfung erforderlich", "Revisão necessária"],
    ["Results", "Resultados", "Résultats", "Ergebnisse", "Resultados"],
    ["Findings", "Observaciones", "Constats", "Befunde", "Observações"],
    ["Details", "Detalles", "Détails", "Details", "Detalhes"],
    ["Transaction", "Transacción", "Transaction", "Transaktion", "Transação"],
    ["History", "Historial", "Historique", "Verlauf", "Histórico"],
    ["No transactions recorded", "No hay transacciones registradas", "Aucune transaction enregistrée", "Keine Transaktionen protokolliert", "Nenhuma transação registrada"],
    ["No preference changes", "Sin cambios de preferencias", "Aucune modification des préférences", "Keine Einstellungsänderungen", "Nenhuma alteração de preferências"],
    ["Requesting administrator access", "Solicitando acceso de administrador", "Demande des droits administrateur", "Administratorrechte werden angefordert", "Solicitando acesso de administrador"],
    ["Press Enter to close", "Pulse Intro para cerrar", "Appuyez sur Entrée pour fermer", "Zum Schließen Eingabetaste drücken", "Pressione Enter para fechar"],
    ["Operation failed", "La operación falló", "Échec de l’opération", "Vorgang fehlgeschlagen", "Falha na operação"],
    ["Invalid command", "Comando no válido", "Commande non valide", "Ungültiger Befehl", "Comando inválido"],
    ["Use --help for usage.", "Use --help para consultar el uso.", "Utilisez --help pour consulter l’aide.", "Verwenden Sie --help für die Hilfe.", "Use --help para consultar a ajuda."],
    ["JSON is available only for audit, apply, revert and history.", "JSON solo está disponible para audit, apply, revert e history.", "JSON est disponible uniquement pour audit, apply, revert et history.", "JSON ist nur für audit, apply, revert und history verfügbar.", "JSON está disponível apenas para audit, apply, revert e history."],
    ["Bitwarden installation requires --yes.", "La instalación de Bitwarden requiere --yes.", "L’installation de Bitwarden nécessite --yes.", "Die Installation von Bitwarden erfordert --yes.", "A instalação do Bitwarden exige --yes."],
    ["Password output requires an interactive terminal.", "La contraseña requiere una terminal interactiva.", "L’affichage du mot de passe nécessite un terminal interactif.", "Die Passwortausgabe erfordert ein interaktives Terminal.", "A exibição da senha exige um terminal interativo."],
    ["New password - save it in your password manager", "Nueva contraseña: guárdela en su gestor de contraseñas", "Nouveau mot de passe - enregistrez-le dans votre gestionnaire", "Neues Passwort - im Passwortmanager speichern", "Nova senha - salve no seu gerenciador de senhas"],
    ["Not saved or copied to the clipboard. Existing passwords were not inspected.", "No se guardó ni se copió al portapapeles. No se inspeccionaron contraseñas existentes.", "Ni enregistré ni copié dans le presse-papiers. Les mots de passe existants n’ont pas été inspectés.", "Nicht gespeichert oder in die Zwischenablage kopiert. Vorhandene Passwörter wurden nicht geprüft.", "Não foi salva nem copiada para a área de transferência. Senhas existentes não foram inspecionadas."],
    ["compliant", "conforme", "conforme", "konform", "conforme"],
    ["attention", "requiere atención", "à vérifier", "Handlungsbedarf", "requer atenção"],
    ["skipped", "omitido", "ignoré", "übersprungen", "ignorado"],
    ["error", "error", "erreur", "Fehler", "erro"],
    ["unknown", "desconocido", "inconnu", "unbekannt", "desconhecido"],
    ["info", "información", "information", "Information", "informação"],
    ["ok", "correcto", "correct", "in Ordnung", "correto"],
    ["pending", "pendiente", "en attente", "ausstehend", "pendente"],
    ["unchanged", "sin cambios", "inchangé", "unverändert", "inalterado"],
    ["conflict", "conflicto", "conflit", "Konflikt", "conflito"],
    ["applied", "aplicado", "appliqué", "angewendet", "aplicado"],
    ["restored", "restaurado", "restauré", "wiederhergestellt", "restaurado"],
    ["reverted", "revertido", "annulé", "zurückgesetzt", "revertido"],
    ["reverting", "revirtiendo", "annulation en cours", "wird zurückgesetzt", "revertendo"],
    ["Defender real-time protection", "Protección en tiempo real de Defender", "Protection en temps réel de Defender", "Defender-Echtzeitschutz", "Proteção em tempo real do Defender"],
    ["Defender behavior monitoring", "Supervisión de comportamiento de Defender", "Surveillance du comportement de Defender", "Defender-Verhaltensüberwachung", "Monitoramento de comportamento do Defender"],
    ["Defender downloaded-file scanning", "Análisis de archivos descargados de Defender", "Analyse des fichiers téléchargés par Defender", "Defender-Prüfung heruntergeladener Dateien", "Verificação de arquivos baixados pelo Defender"],
    ["Defender archive scanning", "Análisis de archivos comprimidos de Defender", "Analyse des archives par Defender", "Defender-Archivprüfung", "Verificação de arquivos compactados pelo Defender"],
    ["Enable firewall", "Activar cortafuegos", "Activer le pare-feu", "Firewall aktivieren", "Ativar firewall"],
    ["Block unsolicited inbound traffic", "Bloquear tráfico entrante no solicitado", "Bloquer le trafic entrant non sollicité", "Unaufgeforderten eingehenden Verkehr blockieren", "Bloquear tráfego de entrada não solicitado"],
    ["domain", "dominio", "domaine", "Domäne", "domínio"],
    ["private", "privado", "privé", "privat", "privado"],
    ["public", "público", "public", "öffentlich", "público"],
    ["Enable UAC", "Activar UAC", "Activer l’UAC", "UAC aktivieren", "Ativar UAC"],
    ["Require administrator consent", "Exigir consentimiento del administrador", "Exiger le consentement administrateur", "Administratorzustimmung verlangen", "Exigir consentimento do administrador"],
    ["Security providers", "Proveedores de seguridad", "Fournisseurs de sécurité", "Sicherheitsanbieter", "Provedores de segurança"],
    ["Windows Firewall", "Cortafuegos de Windows", "Pare-feu Windows", "Windows-Firewall", "Firewall do Windows"],
    ["Windows lifecycle", "Ciclo de vida de Windows", "Cycle de vie de Windows", "Windows-Lebenszyklus", "Ciclo de vida do Windows"],
    ["Device encryption", "Cifrado del dispositivo", "Chiffrement de l’appareil", "Geräteverschlüsselung", "Criptografia do dispositivo"],
    ["Secure Boot", "Arranque seguro", "Démarrage sécurisé", "Sicherer Start", "Inicialização segura"],
    ["Windows updates", "Actualizaciones de Windows", "Mises à jour Windows", "Windows-Updates", "Atualizações do Windows"],
    ["Remote Desktop", "Escritorio remoto", "Bureau à distance", "Remotedesktop", "Área de Trabalho Remota"],
    ["Local accounts", "Cuentas locales", "Comptes locaux", "Lokale Konten", "Contas locais"],
    ["Memory integrity", "Integridad de memoria", "Intégrité de la mémoire", "Speicherintegrität", "Integridade da memória"],
    ["Management and mutation eligibility", "Administración y elegibilidad para cambios", "Gestion et admissibilité aux modifications", "Verwaltung und Änderungsvoraussetzungen", "Gerenciamento e elegibilidade para alterações"],
    ["Journal recovery", "Recuperación del registro", "Récupération du journal", "Journalwiederherstellung", "Recuperação do registro"],
    ["Eligible unmanaged local preference", "Preferencia local no administrada elegible", "Préférence locale non gérée admissible", "Geeignete unverwaltete lokale Einstellung", "Preferência local não gerenciada elegível"],
    ["Preserving absent or nonzero UAC preference", "Se conserva la preferencia UAC ausente o distinta de cero", "Préférence UAC absente ou non nulle conservée", "Fehlende oder von null verschiedene UAC-Einstellung bleibt erhalten", "Preferência UAC ausente ou diferente de zero preservada"],
    ["Revert the active transaction before applying again", "Revierta la transacción activa antes de aplicar de nuevo", "Annulez la transaction active avant de réappliquer", "Aktive Transaktion vor erneuter Anwendung zurücksetzen", "Reverta a transação ativa antes de aplicar novamente"],
    ["Target preference already present; original before image retained", "Preferencia deseada ya presente; estado original conservado", "Préférence cible déjà présente ; état initial conservé", "Zieleinstellung bereits vorhanden; Originalzustand bleibt erhalten", "Preferência desejada já presente; estado original preservado"],
    ["Preference drifted; original before image retained", "Preferencia modificada; estado original conservado", "Préférence modifiée ; état initial conservé", "Einstellung abgewichen; Originalzustand bleibt erhalten", "Preferência alterada; estado original preservado"],
    ["Revert the active transaction before starting another apply", "Revierta la transacción activa antes de iniciar otra aplicación", "Annulez la transaction active avant une nouvelle application", "Aktive Transaktion vor einer weiteren Anwendung zurücksetzen", "Reverta a transação ativa antes de iniciar outra aplicação"],
    ["Target preference already present", "Preferencia deseada ya presente", "Préférence cible déjà présente", "Zieleinstellung bereits vorhanden", "Preferência desejada já presente"],
    ["Preference applied", "Preferencia aplicada", "Préférence appliquée", "Einstellung angewendet", "Preferência aplicada"],
    ["; restart required", "; es necesario reiniciar", "; redémarrage requis", "; Neustart erforderlich", "; reinicialização necessária"],
    ["Original preference already present", "Preferencia original ya presente", "Préférence initiale déjà présente", "Originaleinstellung bereits vorhanden", "Preferência original já presente"],
    ["Original preference restored", "Preferencia original restaurada", "Préférence initiale restaurée", "Originaleinstellung wiederhergestellt", "Preferência original restaurada"],
    ["Preference differs from both target and before image; no write performed", "La preferencia difiere del objetivo y del original; no se escribió", "La préférence diffère de la cible et de l’état initial ; aucune écriture", "Einstellung weicht von Ziel und Original ab; nichts geschrieben", "A preferência difere do destino e do original; nenhuma gravação realizada"],
    ["Preference changed immediately before restore; no write performed", "La preferencia cambió justo antes de restaurar; no se escribió", "Préférence modifiée juste avant la restauration ; aucune écriture", "Einstellung unmittelbar vor Wiederherstellung geändert; nichts geschrieben", "Preferência alterada imediatamente antes da restauração; nenhuma gravação realizada"],
    [" remains unreverted; use revert to restore its recorded preferences.", " sigue sin revertir; use revert para restaurar sus preferencias registradas.", " n’est pas annulée ; utilisez revert pour restaurer ses préférences enregistrées.", " ist noch nicht zurückgesetzt; mit revert die protokollierten Einstellungen wiederherstellen.", " ainda não foi revertida; use revert para restaurar suas preferências registradas."],
    ["Assessment unavailable: ", "Evaluación no disponible: ", "Évaluation indisponible : ", "Bewertung nicht verfügbar: ", "Avaliação indisponível: "],
    ["No management evidence found by conservative local probes. Each mutation repeats management and control-specific capability checks.", "Las comprobaciones locales prudentes no encontraron indicios de administración. Cada cambio repite las comprobaciones de administración y capacidad específicas.", "Les vérifications locales prudentes n’ont détecté aucun indice de gestion. Chaque modification répète les vérifications de gestion et de capacité spécifiques.", "Vorsichtige lokale Prüfungen fanden keine Verwaltungshinweise. Jeder Änderung gehen erneute Verwaltungs- und Funktionsprüfungen voraus.", "As verificações locais prudentes não encontraram indícios de gerenciamento. Cada alteração repete as verificações de gerenciamento e capacidade específicas."],
    ["Review reputation-based protection and SmartScreen in Windows Security and your browser. Per-user, browser and policy settings differ; effective protection is not inferred from a single registry value.", "Revise la protección basada en reputación y SmartScreen en Seguridad de Windows y su navegador. Las configuraciones de usuario, navegador y directivas difieren; un único valor de registro no demuestra protección efectiva.", "Vérifiez la protection fondée sur la réputation et SmartScreen dans Sécurité Windows et votre navigateur. Les paramètres utilisateur, navigateur et stratégie diffèrent ; une seule valeur de registre ne prouve pas la protection effective.", "Reputationsbasierten Schutz und SmartScreen in Windows-Sicherheit und Browser prüfen. Benutzer-, Browser- und Richtlinieneinstellungen unterscheiden sich; ein einzelner Registrierungswert belegt keinen wirksamen Schutz.", "Revise a proteção baseada em reputação e o SmartScreen na Segurança do Windows e no navegador. Configurações de usuário, navegador e política diferem; um único valor de registro não comprova proteção efetiva."],
    // Impact phrases: translation source keys used as Advice::impact values.
    ["A full drive stopping fixes and updates from completing", "Un disco lleno que impide que las correcciones y actualizaciones se completen", "Un disque plein empêchant les correctifs et mises à jour de s'appliquer", "Ein voller Datenträger, der Korrekturen und Updates blockiert", "Um disco cheio que impede correções e atualizações de serem concluídas"],
    ["Malware running as soon as it lands on your PC", "Software malicioso que se ejecuta en cuanto llega a tu PC", "Des maliciels s'exécutant dès leur arrivée sur votre PC", "Schadsoftware, die sofort beim Ankommen auf deinem PC startet", "Software malicioso que executa assim que chega ao seu PC"],
    ["Apps that behave like malware even when not yet known", "Aplicaciones que se comportan como malware aunque no sean conocidas todavía", "Des applications se comportant comme des maliciels même si elles ne sont pas encore connues", "Apps, die sich wie Schadsoftware verhalten, auch wenn sie noch nicht bekannt sind", "Aplicativos que se comportam como malware mesmo sem serem conhecidos ainda"],
    ["Harmful files downloaded from the web or email attachments", "Archivos dañinos descargados de la web o adjuntos en correos", "Des fichiers dangereux téléchargés depuis le web ou en pièces jointes", "Schädliche Dateien aus dem Web oder E-Mail-Anhängen", "Arquivos prejudiciais baixados da web ou em anexos de e-mail"],
    ["Malware hidden inside zip and other compressed files", "Malware oculto dentro de archivos zip y otros comprimidos", "Des maliciels dissimulés dans des archives zip et autres fichiers compressés", "Schadsoftware, die in ZIP- und anderen komprimierten Dateien versteckt ist", "Malware oculto em arquivos zip e outros compactados"],
    ["Other devices on your work network reaching your PC", "Otros dispositivos de tu red de trabajo que acceden a tu PC", "D'autres appareils sur votre réseau professionnel accédant à votre PC", "Andere Geräte im Firmennetzwerk, die auf deinen PC zugreifen", "Outros dispositivos na sua rede de trabalho alcançando seu PC"],
    ["Other devices on your home network reaching your PC", "Otros dispositivos de tu red doméstica que acceden a tu PC", "D'autres appareils sur votre réseau personnel accédant à votre PC", "Andere Geräte im Heimnetzwerk, die auf deinen PC zugreifen", "Outros dispositivos na sua rede doméstica alcançando seu PC"],
    ["Other devices at public places like cafes or airports reaching your PC", "Otros dispositivos en lugares públicos como cafeterías o aeropuertos que acceden a tu PC", "D'autres appareils dans les lieux publics comme les cafés ou aéroports accédant à votre PC", "Andere Geräte an öffentlichen Orten wie Cafés oder Flughäfen, die auf deinen PC zugreifen", "Outros dispositivos em locais públicos como cafés ou aeroportos alcançando seu PC"],
    ["Uninvited incoming connections on your work network", "Conexiones entrantes no solicitadas en tu red de trabajo", "Des connexions entrantes non sollicitées sur votre réseau professionnel", "Unerwünschte eingehende Verbindungen im Firmennetzwerk", "Conexões de entrada não solicitadas na sua rede de trabalho"],
    ["Uninvited incoming connections on your home network", "Conexiones entrantes no solicitadas en tu red doméstica", "Des connexions entrantes non sollicitées sur votre réseau personnel", "Unerwünschte eingehende Verbindungen im Heimnetzwerk", "Conexões de entrada não solicitadas na sua rede doméstica"],
    ["Uninvited incoming connections on public networks like cafes or airports", "Conexiones entrantes no solicitadas en redes públicas como cafeterías o aeropuertos", "Des connexions entrantes non sollicitées sur les réseaux publics comme les cafés ou aéroports", "Unerwünschte eingehende Verbindungen in öffentlichen Netzwerken wie Cafés oder Flughäfen", "Conexões de entrada não solicitadas em redes públicas como cafés ou aeroportos"],
    ["Apps silently making system-wide changes without asking you", "Aplicaciones que hacen cambios en todo el sistema sin pedirte permiso", "Des applications modifiant silencieusement le système sans vous demander", "Apps, die systemweite Änderungen still und heimlich ohne Rückfrage vornehmen", "Aplicativos fazendo alterações em todo o sistema em silêncio sem perguntar"],
    ["Apps making administrator changes without asking for approval", "Aplicaciones que hacen cambios de administrador sin pedir aprobación", "Des applications effectuant des modifications d'administrateur sans demander votre accord", "Apps, die Administratorrechte ohne Genehmigung nutzen", "Aplicativos fazendo alterações de administrador sem pedir aprovação"],
    ["Any app installer quietly getting full control of your PC", "Cualquier instalador que obtiene silenciosamente el control total de tu PC", "Tout programme d'installation obtenant discrètement le contrôle total de votre PC", "Installer, die unbemerkt die vollständige Kontrolle über deinen PC erlangen", "Qualquer instalador obtendo silenciosamente controle total do seu PC"],
    ["Strangers on the network listing your account names to guess passwords", "Personas desconocidas en la red que enumeran tus nombres de cuenta para adivinar contraseñas", "Des inconnus sur le réseau listant vos noms de comptes pour deviner vos mots de passe", "Fremde im Netzwerk, die Kontonamen auflisten, um Passwörter zu erraten", "Desconhecidos na rede listando seus nomes de conta para adivinhar senhas"],
    ["Someone signing in over the network to an account with no password", "Alguien que inicia sesión por la red en una cuenta sin contraseña", "Quelqu'un se connectant via le réseau à un compte sans mot de passe", "Jemand, der sich über das Netzwerk bei einem Konto ohne Passwort anmeldet", "Alguém fazendo login pela rede em uma conta sem senha"],
    ["Attackers stealing your Windows password from memory", "Atacantes que roban tu contraseña de Windows de la memoria", "Des attaquants volant votre mot de passe Windows depuis la mémoire", "Angreifer, die dein Windows-Passwort aus dem Arbeitsspeicher stehlen", "Invasores roubando sua senha do Windows da memória"],
    ["Tampered or fake Windows updates reaching your PC", "Actualizaciones de Windows falsas o manipuladas que llegan a tu PC", "Des mises à jour Windows falsifiées ou corrompues atteignant votre PC", "Manipulierte oder gefälschte Windows-Updates auf deinem PC", "Atualizações do Windows falsas ou adulteradas chegando ao seu PC"],
    ["Running Windows that no longer gets security fixes", "Usar una versión de Windows que ya no recibe correcciones de seguridad", "Utiliser une version de Windows qui ne reçoit plus de correctifs de sécurité", "Eine Windows-Version nutzen, die keine Sicherheits-Updates mehr erhält", "Usar uma versão do Windows que não recebe mais correções de segurança"],
    ["Strangers reading your files if your PC is lost or stolen", "Desconocidos que leen tus archivos si tu PC se pierde o es robada", "Des inconnus lisant vos fichiers si votre PC est perdu ou volé", "Fremde, die deine Dateien lesen, wenn dein PC verloren geht oder gestohlen wird", "Desconhecidos lendo seus arquivos se o seu PC for perdido ou roubado"],
    ["Hidden malware loading before Windows starts", "Malware oculto que se carga antes de que arranque Windows", "Des maliciels cachés se chargeant avant le démarrage de Windows", "Versteckte Schadsoftware, die vor dem Windows-Start geladen wird", "Malware oculto carregando antes do Windows iniciar"],
    ["Known security holes staying open on your PC", "Vulnerabilidades de seguridad conocidas que permanecen abiertas en tu PC", "Des failles de sécurité connues restant ouvertes sur votre PC", "Bekannte Sicherheitslücken, die auf deinem PC offen bleiben", "Falhas de segurança conhecidas permanecendo abertas no seu PC"],
    ["Attackers trying to sign in to your PC remotely", "Atacantes que intentan iniciar sesión en tu PC de forma remota", "Des attaquants essayant de se connecter à votre PC à distance", "Angreifer, die versuchen, sich aus der Ferne an deinem PC anzumelden", "Invasores tentando fazer login no seu PC remotamente"],
    ["Old file-sharing flaws used by worms like WannaCry", "Fallos antiguos en el uso compartido de archivos usados por gusanos como WannaCry", "Des failles anciennes de partage de fichiers exploitées par des vers comme WannaCry", "Alte Dateifreigabe-Schwachstellen, die von Würmern wie WannaCry ausgenutzt werden", "Falhas antigas de compartilhamento de arquivos usadas por worms como WannaCry"],
    ["Scam websites and unrecognized apps you open by mistake", "Sitios web fraudulentos y aplicaciones no reconocidas que abres por error", "Des sites frauduleux et des applications non reconnues que vous ouvrez par erreur", "Betrügerische Websites und unbekannte Apps, die du aus Versehen öffnest", "Sites fraudulentos e aplicativos não reconhecidos que você abre por engano"],
    ["Weak or shared sign-ins that are easier to guess or steal", "Cuentas con contraseñas débiles o compartidas más fáciles de adivinar o robar", "Des identifiants faibles ou partagés plus faciles à deviner ou voler", "Schwache oder gemeinsam genutzte Anmeldedaten, die leichter zu erraten oder stehlen sind", "Logins fracos ou compartilhados mais fáceis de adivinhar ou roubar"],
    ["Malicious drivers taking over the core of Windows", "Controladores maliciosos que toman el control del núcleo de Windows", "Des pilotes malveillants prenant le contrôle du cœur de Windows", "Schädliche Treiber, die den Windows-Kern übernehmen", "Drivers maliciosos assumindo o controle do núcleo do Windows"],
    ["Anyone who turns on your PC getting straight into your account", "Cualquiera que encienda tu PC y acceda directamente a tu cuenta", "Toute personne allumant votre PC accédant directement à votre compte", "Jeder, der deinen PC einschaltet, gelangt direkt in dein Konto", "Qualquer pessoa que ligue seu PC acessando diretamente sua conta"],
    // Impact prefix keys
    ["Risk:", "Riesgo:", "Risque :", "Risiko:", "Risco:"],
    ["Protects you from:", "Te protege de:", "Vous protège contre :", "Schützt dich vor:", "Protege você de:"],
    ["Why it matters:", "Por qué importa:", "Pourquoi c'est important :", "Warum das wichtig ist:", "Por que isso importa:"],
    // Column header
    ["Why it matters / Next step", "Por qué importa / Próximo paso", "Pourquoi c'est important / Prochaine étape", "Warum das wichtig ist / Nächster Schritt", "Por que isso importa / Próxima etapa"],
    // Payoff section headings
    ["You're now protected from:", "Ahora estás protegido frente a:", "Vous êtes désormais protégé contre :", "Du bist jetzt geschützt vor:", "Agora você está protegido de:"],
    ["After you restart, you'll be protected from:", "Tras reiniciar, estarás protegido frente a:", "Après redémarrage, vous serez protégé contre :", "Nach dem Neustart bist du geschützt vor:", "Após reiniciar, você estará protegido de:"],
    // Recap note for restart-required fixes
    ["Needs a restart to finish", "Necesita un reinicio para finalizar", "Nécessite un redémarrage pour terminer", "Benötigt einen Neustart zum Abschluss", "Precisa de uma reinicialização para concluir"],
];

// Source-keyed Italian catalog. Coverage tests require an exact match with TEXT.
#[rustfmt::skip]
const ITALIAN: &[[&str; 2]] = &[
    ["{protected} of {total} checks protected", "{protected} di {total} controlli protetti"],
    ["effective protection is unverified; pending transaction", "la protezione effettiva non è verificata; transazione in sospeso"],
    ["Repair readiness blocks new changes", "Le condizioni del dispositivo impediscono nuove modifiche"],
    ["Firewall authority is unavailable", "Impossibile determinare chi gestisce il firewall"],
    ["Effective firewall evidence is unavailable", "I dati sullo stato effettivo del firewall non sono disponibili"],
    ["Firewall evidence contradicts the local preference", "I dati del firewall contraddicono l'impostazione locale"],
    ["Firewall evidence is invalid for this control", "I dati del firewall non sono validi per questo controllo"],
    ["Firewall evidence does not match the control", "I dati del firewall non corrispondono al controllo"],
    ["Nonlocal firewall authority cannot be eligible", "La gestione non locale del firewall non consente modifiche"],
    ["Relevant policy is configured: assessment only", "È configurato un criterio pertinente: solo valutazione"],
    ["EffectiveFirewallMismatch", "Lo stato effettivo del firewall non corrisponde all'impostazione salvata"],
    ["EffectiveFirewallUnavailable", "Non è stato possibile verificare lo stato effettivo del firewall"],
    ["Firewall stored profile cannot be established", "Impossibile determinare il profilo salvato del firewall"],
    ["Firewall inbound preference is not readable", "L'impostazione del traffico in entrata del firewall non è leggibile"],
    ["Unknown firewall control", "Controllo del firewall sconosciuto"],
    ["Firewall effective profile cannot be established", "Impossibile determinare il profilo effettivo del firewall"],
    ["Firewall enabled value must be a boolean", "Il valore di attivazione del firewall deve essere booleano"],
    ["Fix recommended", "Applica le correzioni consigliate"],
    ["The new check could not finish. Check again before choosing more fixes. Undo is still available.", "La nuova verifica non è terminata. Ripeti il controllo prima di scegliere altre correzioni. Puoi ancora annullare le modifiche."],
    ["Only these fixes will be applied. Some changes may need a restart. Extra tools and software installs are not included.", "Verranno applicate solo queste correzioni. Alcune modifiche potrebbero richiedere un riavvio. Gli strumenti aggiuntivi e le installazioni software non sono inclusi."],
    ["Apply these fixes?", "Applicare queste correzioni?"],
    ["Apply these fixes", "Applica queste correzioni"],
    ["Change selection", "Cambia selezione"],
    ["Choose the fixes to keep. The boxes start unchecked.", "Scegli le correzioni da mantenere. All'inizio nessuna casella è selezionata."],
    ["Rechecking your protection", "Nuova verifica della tua protezione"],
    ["Technical details of the latest check failure:", "Dettagli tecnici dell'ultima verifica non riuscita:"],
    ["Protected by Windows", "Protetto da Windows"],
    ["Windows is already providing this firewall protection. No change is needed.", "Windows offre già questa protezione del firewall. Non serve alcuna modifica."],
    ["The active firewall setting could not be verified. Check again before making changes.", "Non è stato possibile verificare l'impostazione attiva del firewall. Ripeti il controllo prima di fare modifiche."],
    ["For your information", "Per tua informazione"],
    ["More information", "Altre informazioni"],
    ["Device check", "Controllo del dispositivo"],
    ["Before making changes", "Prima di fare modifiche"],
    ["Windows drive", "Unità di Windows"],
    ["Saved changes drive", "Unità delle modifiche salvate"],
    ["Free space unknown", "Spazio libero sconosciuto"],
    ["{gb} GB free", "{gb} GB liberi"],
    ["Disk is read-only. Fixes will wait.", "Il disco è in sola lettura. Le correzioni dovranno attendere."],
    ["No space for saved changes. Fixes will wait.", "Non c'è spazio per salvare le modifiche. Le correzioni dovranno attendere."],
    ["Power information unknown", "Informazioni sull'alimentazione sconosciute"],
    ["Plugged in", "Collegato alla corrente"],
    ["Not plugged in", "Non collegato alla corrente"],
    ["Power source unknown", "Fonte di alimentazione sconosciuta"],
    ["Battery: not applicable", "Batteria: non applicabile"],
    ["Battery: {percent}%", "Batteria: {percent}%"],
    ["Low battery. Connect power before making changes.", "Batteria scarica. Collega il dispositivo alla corrente prima di fare modifiche."],
    ["Battery level unknown", "Livello della batteria sconosciuto"],
    ["Battery information unknown", "Informazioni sulla batteria sconosciute"],
    ["Windows Update needs a restart. Save your work and restart when ready.", "Windows Update richiede un riavvio. Salva il tuo lavoro e riavvia quando sei pronto."],
    ["No update restart pending", "Nessun riavvio in attesa per gli aggiornamenti"],
    ["Update restart status unknown", "Stato del riavvio per gli aggiornamenti sconosciuto"],
    ["Readiness evidence", "Dati sulla preparazione del dispositivo"],
    ["Firewall evidence", "Dati del firewall"],
    ["Keep Secblitz up to date", "Mantieni Secblitz aggiornato"],
    ["Check for Secblitz updates", "Cerca aggiornamenti per Secblitz"],
    ["Show the latest update status", "Mostra l'ultimo stato degli aggiornamenti"],
    ["Output raw JSON reports (including update check/status)", "Mostra rapporti JSON non tradotti (inclusi update check/status)"],
    ["JSON is available only for audit, apply, revert, history, update check and update status.", "JSON è disponibile solo per audit, apply, revert, history, update check e update status."],
    ["No update information yet.", "Non ci sono ancora informazioni sugli aggiornamenti."],
    ["Updates aren't available for this installation.", "Gli aggiornamenti non sono disponibili per questa installazione."],
    ["Secblitz is up to date.", "Secblitz è aggiornato."],
    ["We'll try updating when Secblitz is closed.", "Riproveremo ad aggiornare quando Secblitz sarà chiuso."],
    ["Your update is ready. Close Secblitz so the installer can continue.", "L'aggiornamento è pronto. Chiudi Secblitz per consentire all'installazione di continuare."],
    ["Secblitz was updated.", "Secblitz è stato aggiornato."],
    ["The update could not be completed.", "Non è stato possibile completare l'aggiornamento."],
    ["Run update commands from an administrator terminal.", "Esegui i comandi di aggiornamento da un terminale amministratore."],
    ["Run update status --details from an administrator terminal for more information.", "Esegui update status --details da un terminale amministratore per maggiori informazioni."],
    ["Terminal is too short to display a menu", "La finestra del terminale è troppo bassa per mostrare il menu"],
    ["Press Enter to confirm, or Esc to cancel.", "Premi Invio per confermare oppure Esc per annullare."],
    ["Invalid menu default", "Scelta predefinita del menu non valida"],
    ["Invalid menu selection", "Selezione del menu non valida"],
    ["validated menu index", "indice del menu convalidato"],
    ["Review protection and next steps", "Guarda la protezione e i prossimi passi"],
    ["Check my PC again", "Ricontrolla il mio PC"],
    ["Undo my last fixes", "Annulla le mie ultime correzioni"],
    ["Extra tools", "Altri strumenti"],
    ["Technical details (optional)", "Dettagli tecnici (facoltativi)"],
    ["Exit", "Esci"],
    ["Back", "Indietro"],
    ["Generate a password", "Crea una password"],
    ["Install Bitwarden (optional password manager)", "Installa Bitwarden (gestore di password facoltativo)"],
    ["Install and start optional monitoring", "Installa e avvia il monitoraggio facoltativo"],
    ["Update Microsoft Defender protection", "Aggiorna la protezione di Microsoft Defender"],
    ["Run a Microsoft Defender quick scan", "Esegui un'analisi rapida con Microsoft Defender"],
    ["Open Windows Update settings", "Apri le impostazioni di Windows Update"],
    ["Open Windows Security settings", "Apri Sicurezza di Windows"],
    ["Open device encryption / BitLocker settings", "Apri la crittografia del dispositivo / BitLocker"],
    ["Open sign-in settings", "Apri le opzioni di accesso"],
    ["Use ↑/↓ to move, Enter to choose, Esc to go back.", "Usa ↑/↓ per spostarti, Invio per scegliere, Esc per tornare indietro."],
    ["Use ↑/↓ to move, Space to select, Enter to continue, Esc to cancel.", "Usa ↑/↓ per spostarti, Spazio per selezionare, Invio per continuare, Esc per annullare."],
    ["Yes, continue", "Sì, continua"],
    ["No, go back", "No, torna indietro"],
    ["Choose an action", "Scegli un'azione"],
    ["Choose what to fix", "Scegli cosa correggere"],
    ["Select the fixes you want.", "Seleziona le correzioni che vuoi."],
    ["Open Windows Update settings now?", "Aprire ora le impostazioni di Windows Update?"],
    ["Open Windows Security settings now?", "Aprire ora Sicurezza di Windows?"],
    ["Open device encryption settings now?", "Aprire ora le impostazioni di crittografia del dispositivo?"],
    ["Open sign-in settings now?", "Aprire ora le opzioni di accesso?"],
    ["[1] Choose what to fix", "[1] Scegli cosa correggere"],
    ["[3] Check my PC again", "[3] Ricontrolla il mio PC"],
    ["Desktop requests require a non-elevated window", "Le richieste al desktop richiedono una finestra senza privilegi di amministratore"],
    ["That action did not finish. You can view the details before trying again.", "L'azione non è terminata. Puoi leggere i dettagli prima di riprovare."],
    ["Return to the PC check? Windows will ask for administrator permission again.", "Tornare alla verifica del PC? Windows chiederà di nuovo l'autorizzazione come amministratore."],
    ["To open Settings from here, close Secblitz and open it normally, without Run as administrator.", "Per aprire Impostazioni da qui, chiudi Secblitz e aprilo normalmente, senza Esegui come amministratore."],
    ["Return to your original Secblitz window to open Settings? That window will ask you again before opening anything.", "Tornare alla finestra originale di Secblitz per aprire Impostazioni? Ti verrà chiesto di nuovo il consenso prima di aprire qualsiasi pagina."],
    ["Unexpected action result: ", "Risultato inatteso dell'azione: "],
    ["Worker panicked without a text payload", "Il processo di lavoro si è interrotto senza un messaggio di testo"],
    ["Action worker failed: ", "Esecuzione dell'azione non riuscita: "],
    ["Extra actions are not part of Undo my last fixes.", "Le azioni aggiuntive non sono incluse in Annulla le mie ultime correzioni."],
    ["Choose Check again when you are ready to verify current protection.", "Scegli Ricontrolla il mio PC quando vuoi verificare la protezione attuale."],
    ["Select at least one control", "Seleziona almeno una misura"],
    ["Duplicate selected control: ", "Misura selezionata più volte: "],
    ["Unknown selected control: ", "Misura selezionata sconosciuta: "],
    ["Duplicate active control owner; revert before applying", "Una misura appartiene a più transazioni attive; annulla le modifiche prima di applicarne altre"],
    ["Selected batch blocked by an owned control conflict or probe failure", "Il gruppo selezionato è bloccato da un conflitto su una misura già registrata o da una verifica non riuscita"],
    ["Incomplete transaction precedes another active transaction", "Una transazione incompleta precede un'altra transazione attiva"],
    ["Duplicate active control owner; journal history is invalid", "Una misura appartiene a più transazioni attive; la cronologia del registro delle modifiche non è valida"],
    ["Unknown settings URI", "URI delle impostazioni sconosciuto"],
    ["Open Settings from the non-elevated interactive application", "Apri le Impostazioni dall'applicazione interattiva senza privilegi elevati"],
    ["Settings actions require Windows", "Le azioni sulle impostazioni richiedono Windows"],
    ["Windows could not open settings", "Windows non ha potuto aprire le impostazioni"],
    ["Windows accepted the settings-page request. Page availability and security settings are not verified; no fix is claimed.", "Windows ha accettato la richiesta di apertura delle impostazioni. La disponibilità della pagina e le impostazioni di sicurezza non sono verificate; non viene dichiarata alcuna correzione."],
    ["Defender's signature-update command returned successfully using its configured sources. This does not establish that signatures are the latest available.", "Il comando di aggiornamento delle firme di Defender è terminato senza errori usando le fonti configurate. Questo non conferma che le firme siano le più recenti disponibili."],
    ["Defender's quick-scan command returned successfully. Completion and threat status are not independently verified; review Windows Security for results.", "Il comando di analisi rapida di Defender è terminato senza errori. Il completamento dell'analisi e lo stato delle minacce non sono verificati indipendentemente; consulta i risultati in Sicurezza di Windows."],
    ["Monitor startup failed; the installed service was retained. Check service status before retrying", "Avvio del monitor non riuscito; il servizio installato è stato mantenuto. Controlla lo stato del servizio prima di riprovare"],
    ["SCM reports SecblitzMonitor Running. Monitoring is read-only; this does not verify report freshness or machine health.", "SCM indica che SecblitzMonitor è in esecuzione. Il monitoraggio è in sola lettura; questo non verifica l'aggiornamento dei rapporti né lo stato del computer."],
    ["Defender support actions require Windows", "Le azioni di manutenzione di Defender richiedono Windows"],
    ["Unknown support action id", "Identificativo dell'azione di manutenzione sconosciuto"],
    ["Embedded backend dispatcher boundary changed", "Il limite del dispatcher del motore incorporato è cambiato"],
    ["Ambiguous embedded backend dispatcher boundary", "Limite del dispatcher del motore incorporato ambiguo"],
    ["Defender support actions require Administrator elevation", "Le azioni di manutenzione di Defender richiedono privilegi di amministratore"],
    ["Defender support action failed; work may continue in Defender. Review Windows Security; no completion or rollback is assumed", "L'azione di manutenzione di Defender non è riuscita; Defender potrebbe continuare a lavorare. Controlla Sicurezza di Windows; non si presume né il completamento né l'annullamento"],
    ["Defender command return was not acknowledged; review Windows Security", "Il termine del comando di Defender non è stato confermato; controlla Sicurezza di Windows"],
    ["Service startup requires Administrator elevation", "L'avvio del servizio richiede privilegi di amministratore"],
    ["Cannot inspect service security", "Impossibile esaminare la sicurezza del servizio"],
    ["Monitor cannot start from SCM state ", "Il monitor non può avviarsi dallo stato SCM "],
    ["Monitor did not reach Running: ", "Il monitor non ha raggiunto lo stato In esecuzione: "],
    ["Timed out waiting for SCM Running; the monitor may still start. Installation was retained", "Tempo scaduto in attesa dello stato In esecuzione di SCM; il monitor potrebbe ancora avviarsi. L'installazione è stata mantenuta"],
    ["Unexpected service configuration; refusing to start", "Configurazione del servizio inattesa; avvio rifiutato"],
    ["Cannot inspect service owner", "Impossibile verificare il proprietario del servizio"],
    ["Untrusted service owner", "Proprietario del servizio non attendibile"],
    ["Missing or invalid service DACL", "DACL del servizio assente o non valida"],
    ["Unprotected service DACL", "DACL del servizio non protetta"],
    ["Cannot inspect service ACE", "Impossibile esaminare la ACE del servizio"],
    ["Unexpected service ACE", "ACE del servizio inattesa"],
    ["Invalid service trustee", "Destinatario delle autorizzazioni del servizio non valido"],
    ["Unexpected service trustee", "Destinatario delle autorizzazioni del servizio inatteso"],
    ["Unexpected service permissions", "Autorizzazioni del servizio inattese"],
    ["Missing service trustees", "Destinatari delle autorizzazioni del servizio mancanti"],
    ["Unknown support operation", "Operazione di manutenzione sconosciuta"],
    ["Domain-managed or unknown membership: support action declined", "Gestione tramite dominio o appartenenza sconosciuta: azione di manutenzione rifiutata"],
    ["MDM-managed device: support action declined", "Dispositivo gestito tramite MDM: azione di manutenzione rifiutata"],
    ["Enrollment or cloud-management evidence: support action declined", "Rilevata registrazione o gestione tramite cloud: azione di manutenzione rifiutata"],
    ["Local policy artifacts: support action declined", "Rilevati elementi di criteri locali: azione di manutenzione rifiutata"],
    ["Additional, missing or unrecognized antivirus provider: support action declined", "Prodotto antivirus aggiuntivo, assente o non riconosciuto: azione di manutenzione rifiutata"],
    ["Defender is not confirmed active in Normal mode", "Non è confermato che Defender sia attivo in modalità Normal"],
    ["Install Bitwarden in your original desktop account now?", "Installare ora Bitwarden nel tuo account desktop originale?"],
    ["Bitwarden installation did not finish. Review the failure before trying again.", "L'installazione di Bitwarden non è terminata. Controlla l'errore prima di riprovare."],
    ["Show technical details of this failure?", "Mostrare i dettagli tecnici di questo errore?"],
    ["A safer PC. Without headaches.", "Un PC più sicuro. Senza mal di testa."],
    ["No command: open the guided security check. Nothing is fixed without your choice.", "Senza comando: apre la verifica guidata della sicurezza. Scegli tu cosa correggere."],
    ["Show technical report details", "Mostra i dettagli tecnici del rapporto"],
    ["Guided security check and selected fixes", "Verifica guidata e correzioni a tua scelta"],
    ["Start the optional monitoring service", "Avvia il servizio facoltativo di monitoraggio"],
    ["Monitoring is running. Check reports separately to verify their freshness.", "Il monitoraggio è attivo. Consulta i rapporti per verificare che siano aggiornati."],
    ["The operation could not be completed. Run again with --details to see technical information.", "Non è stato possibile completare l'operazione. Ripeti con --details per vedere le informazioni tecniche."],
    ["Saved changes are available for review or undo. This does not mean every requested fix completed.", "Puoi esaminare o annullare le modifiche salvate. Questo non significa che tutte le correzioni richieste siano state completate."],
    ["The guided check needs an interactive terminal. Open a terminal and run secblitz guide, or use secblitz audit --json for a report.", "La verifica guidata richiede un terminale interattivo. Apri un terminale ed esegui secblitz guide, oppure usa secblitz audit --json per ottenere un rapporto."],
    ["[1] Yes  [0] No (default)", "[1] Sì  [0] No (predefinito)"],
    ["First, we will check your protection. You choose what to fix; checking does not apply fixes.", "Prima controlliamo la tua protezione. Scegli tu cosa correggere: la verifica non applica correzioni."],
    ["[1] Fix selected recommended items", "[1] Correggi quello che scelgo"],
    ["[2] Review protection and next steps", "[2] Guarda la protezione e i prossimi passi"],
    ["[3] Check again", "[3] Ricontrolla il mio PC"],
    ["[4] Undo my last fixes", "[4] Annulla le mie ultime correzioni"],
    ["[5] Extra tools", "[5] Altri strumenti"],
    ["[6] Technical details (optional)", "[6] Dettagli tecnici (facoltativi)"],
    ["[0] Exit", "[0] Esci"],
    ["Check again before choosing fixes. The previous check is no longer current.", "Ripeti la verifica prima di scegliere le correzioni. Il controllo precedente non è più aggiornato."],
    ["There are no recommended automatic fixes available. See details for other next steps.", "Non ci sono correzioni automatiche consigliate disponibili. Leggi i dettagli per sapere cos'altro puoi fare."],
    ["Choose only the items you want to fix:", "Scegli solo quello che vuoi correggere:"],
    ["all", "tutti"],
    ["none", "nessuno"],
    ["complete", "completato"],
    ["opened", "aperto"],
    ["returned", "comando terminato"],
    ["running", "in esecuzione"],
    ["Enter numbers separated by commas, ranges such as 1-3, all, or none. Enter alone selects nothing.", "Inserisci numeri separati da virgole, intervalli come 1-3, tutti oppure nessuno. Premere solo Invio non seleziona nulla."],
    ["That selection is not valid. Use only the displayed numbers, all, or none.", "La selezione non è valida. Usa solo i numeri mostrati, tutti oppure nessuno."],
    ["Nothing selected. No changes made.", "Non hai selezionato nulla. Nessuna modifica effettuata."],
    ["Review your selected fixes:", "Controlla le correzioni che hai scelto:"],
    ["Apply these selected fixes now? Some changes may need a restart.", "Applicare ora queste correzioni? Alcune modifiche potrebbero richiedere un riavvio."],
    ["Check again to review current protection, or undo your recorded fixes.", "Ripeti la verifica della protezione attuale oppure annulla le correzioni registrate."],
    ["Undo the latest recorded fixes? This restores their saved original settings; extra tools and software installs are not undone.", "Annullare le ultime correzioni registrate? Verranno ripristinate le impostazioni originali salvate; gli strumenti aggiuntivi e le installazioni software non vengono annullati."],
    ["Technical details of the last failure (may include system paths and native messages):", "Dettagli tecnici dell'ultimo errore (possono includere percorsi di sistema e messaggi nativi):"],
    ["Technical details of the last completed operation (not a new protection check):", "Dettagli tecnici dell'ultima operazione completata (non è una nuova verifica della protezione):"],
    ["Choose one of the displayed menu numbers.", "Scegli uno dei numeri del menu."],
    ["The operation did not finish. Some changes may already have been made; remaining work is not confirmed. You can check again or undo recorded fixes.", "L'operazione non è terminata. Alcune modifiche potrebbero essere già state applicate; il resto non è confermato. Puoi ripetere la verifica o annullare le correzioni registrate."],
    ["Choose Technical details to see the original failure.", "Scegli Dettagli tecnici per vedere l'errore originale."],
    ["[1] Generate a password", "[1] Genera una password"],
    ["[2] Install Bitwarden (optional password manager)", "[2] Installa Bitwarden (gestore di password facoltativo)"],
    ["[3] Install and start optional monitoring", "[3] Installa e avvia il monitoraggio facoltativo"],
    ["[4] Update Microsoft Defender protection", "[4] Aggiorna la protezione di Microsoft Defender"],
    ["[5] Run a Microsoft Defender quick scan", "[5] Esegui un'analisi rapida con Microsoft Defender"],
    ["[0] Back", "[0] Indietro"],
    ["Return to your original non-administrator window to install Bitwarden? That window will ask for consent again.", "Tornare alla finestra originale senza privilegi di amministratore per installare Bitwarden? Ti verrà chiesto nuovamente il consenso."],
    ["Bitwarden must be installed from your normal, non-administrator desktop terminal. Open that terminal and run secblitz tools bitwarden --yes after reviewing the installation consent in --help.", "Bitwarden va installato dal tuo normale terminale desktop, senza privilegi di amministratore. Aprilo ed esegui secblitz tools bitwarden --yes dopo aver letto il consenso all'installazione in --help."],
    ["Opening settings does not fix a finding. Follow the Windows instructions, then check again.", "Aprire le impostazioni non risolve il problema. Segui le istruzioni di Windows, poi ripeti la verifica."],
    ["[1] Open Windows Update settings", "[1] Apri le impostazioni di Windows Update"],
    ["[2] Open Windows Security settings", "[2] Apri le impostazioni di Sicurezza di Windows"],
    ["[3] Open device encryption / BitLocker settings", "[3] Apri le impostazioni di crittografia del dispositivo / BitLocker"],
    ["[4] Open sign-in settings", "[4] Apri le opzioni di accesso"],
    ["[5] Update Microsoft Defender protection", "[5] Aggiorna la protezione di Microsoft Defender"],
    ["[6] Run a Microsoft Defender quick scan", "[6] Esegui un'analisi rapida con Microsoft Defender"],
    ["Defender will connect to its configured update sources and download protection updates.", "Defender si collegherà alle fonti di aggiornamento configurate e scaricherà gli aggiornamenti della protezione."],
    ["Defender will scan your device and may remediate threats using your existing Defender settings. This can take several minutes.", "Defender analizzerà il dispositivo e potrà intervenire sulle minacce secondo le impostazioni attuali. Potrebbero servire diversi minuti."],
    ["This installs the optional read-only monitor if needed and starts it. It does not automatically fix findings.", "Installa, se necessario, il monitor facoltativo in sola lettura e lo avvia. Non corregge automaticamente i problemi rilevati."],
    ["Open settings only: no fix is applied or verified by opening this page.", "Apre solo le impostazioni: l'apertura di questa pagina non applica né verifica alcuna correzione."],
    ["Run the selected extra action now? Extra actions are not part of Undo my last fixes.", "Eseguire ora l'azione aggiuntiva scelta? Le azioni aggiuntive non sono incluse in Annulla le mie ultime correzioni."],
    ["Settings opened. Follow the Windows instructions; opening settings does not mean the issue is fixed.", "Impostazioni aperte. Segui le istruzioni di Windows: averle aperte non significa che il problema sia risolto."],
    ["Defender's command returned. Review Windows Security for update or scan results, then check again.", "Il comando di Defender è terminato. Consulta i risultati dell'aggiornamento o dell'analisi in Sicurezza di Windows, poi ripeti la verifica."],
    ["The action could not be verified. Defender work may still be running. Review Windows Security or service status before trying again.", "Non è stato possibile verificare l'azione. Defender potrebbe essere ancora al lavoro. Controlla Sicurezza di Windows o lo stato del servizio prima di riprovare."],
    ["Open Windows Security and review Virus & threat protection.", "Apri Sicurezza di Windows e controlla Protezione da virus e minacce."],
    ["Open Windows Security and review Firewall & network protection.", "Apri Sicurezza di Windows e controlla Firewall e protezione rete."],
    ["Review User Account Control settings with your administrator.", "Controlla le impostazioni di Controllo dell'account utente con il tuo amministratore."],
    ["Ask your administrator to review app installation permissions.", "Chiedi al tuo amministratore di controllare i permessi per installare le app."],
    ["Ask your administrator to review anonymous access to account names.", "Chiedi al tuo amministratore di controllare l'accesso anonimo ai nomi degli account."],
    ["Review account passwords and remote sign-in access with your administrator.", "Controlla le password degli account e gli accessi remoti con il tuo amministratore."],
    ["Ask your administrator to review how Windows keeps sign-in secrets.", "Chiedi al tuo amministratore di controllare come Windows conserva i dati di accesso."],
    ["Ask your administrator to review update service permissions.", "Chiedi al tuo amministratore di controllare i permessi dei servizi di aggiornamento."],
    ["View details and run the check again before deciding what to change.", "Leggi i dettagli e ripeti la verifica prima di decidere cosa cambiare."],
    ["Secblitz can fix this. Help protect against uninvited connections.", "Secblitz può correggere questa impostazione e aiutarti a bloccare le connessioni indesiderate."],
    ["Secblitz can fix this. Help protect updates from tampering.", "Secblitz può correggere questa impostazione e aiutarti a proteggere gli aggiornamenti dalle manomissioni."],
    ["Secblitz can fix this. Stop keeping reusable sign-in secrets after a restart.", "Secblitz può correggere questa impostazione. Dopo un riavvio, i dati di accesso riutilizzabili non verranno più conservati."],
    ["Secblitz can fix this. Restore permission prompts after a restart.", "Secblitz può correggere questa impostazione. Dopo un riavvio, le richieste di autorizzazione torneranno attive."],
    ["Secblitz can fix this. Ask for approval before administrator changes.", "Secblitz può correggere questa impostazione e richiedere il consenso prima delle modifiche da amministratore."],
    ["Secblitz can fix this. Limit elevated permissions for app installers.", "Secblitz può correggere questa impostazione e limitare i privilegi elevati dei programmi di installazione."],
    ["Secblitz can fix this. Limit anonymous access to account names.", "Secblitz può correggere questa impostazione e limitare l'accesso anonimo ai nomi degli account."],
    ["Secblitz can fix this. Restrict remote sign-ins with blank passwords.", "Secblitz può correggere questa impostazione e limitare gli accessi remoti con password vuote."],
    ["Secblitz can fix this. Turn on this virus protection setting.", "Secblitz può correggere questa impostazione e attivare questa protezione antivirus."],
    ["No action needed for this check.", "Per questa verifica non devi fare nulla."],
    ["This setting was updated and checked.", "Questa impostazione è stata aggiornata e verificata."],
    ["Your earlier setting was restored.", "La tua impostazione precedente è stata ripristinata."],
    ["Review saved changes and finish undo before making more changes.", "Controlla le modifiche salvate e completa l'annullamento prima di farne altre."],
    ["This setting changed since it was saved. Review details before undoing it.", "Questa impostazione è cambiata dopo il salvataggio. Leggi i dettagli prima di annullarla."],
    ["Ask the person or organization managing this PC to review this setting.", "Chiedi alla persona o all'organizzazione che gestisce questo PC di controllare l'impostazione."],
    ["Kept your existing setting. It may already protect you or use Windows defaults; review details if unsure.", "La tua impostazione è stata mantenuta. Potrebbe già proteggerti o usare i valori predefiniti di Windows; se hai dubbi, leggi i dettagli."],
    ["Save your work and restart your PC to finish this change.", "Salva il tuo lavoro e riavvia il PC per completare questa modifica."],
    ["Open Windows Security to check which security app is active and healthy.", "Apri Sicurezza di Windows per vedere quale app di sicurezza è attiva e funziona correttamente."],
    ["Open Windows Security to review virus protection and protection updates.", "Apri Sicurezza di Windows per controllare la protezione antivirus e i suoi aggiornamenti."],
    ["Check support for your Windows version and edition, including any extended support plan.", "Verifica il supporto della tua versione ed edizione di Windows, compresi eventuali piani di supporto esteso."],
    ["Review device encryption and save your recovery key before changing encryption settings.", "Controlla la crittografia del dispositivo e salva la chiave di ripristino prima di cambiare le impostazioni."],
    ["Check your PC maker's Secure Boot instructions before changing firmware settings.", "Consulta le istruzioni del produttore sull'Avvio protetto prima di cambiare le impostazioni del firmware."],
    ["Open Windows Update and check for updates. An offline check cannot confirm you are up to date.", "Apri Windows Update e cerca gli aggiornamenti. Una verifica offline non può confermare che sia tutto aggiornato."],
    ["Review Remote Desktop in Settings. Turn it off if you do not use it.", "Controlla Desktop remoto nelle Impostazioni. Disattivalo se non lo usi."],
    ["Review older device dependencies before turning off SMB1 in Windows Features.", "Verifica se qualche vecchio dispositivo richiede SMB1 prima di disattivarlo in Funzionalità Windows."],
    ["Review reputation-based protection in Windows Security and your browser.", "Controlla la protezione basata sulla reputazione in Sicurezza di Windows e nel browser."],
    ["Review who can sign in. Use unique passwords and extra sign-in verification where supported.", "Controlla chi può accedere. Usa password uniche e una verifica aggiuntiva dell'accesso, dove disponibile."],
    ["Review Core isolation in Windows Security and driver compatibility before enabling memory integrity.", "Controlla Isolamento core in Sicurezza di Windows e la compatibilità dei driver prima di attivare Integrità della memoria."],
    ["Review work or school connections in Settings if you are unsure who manages this PC.", "Se non sai chi gestisce questo PC, controlla i collegamenti ad account aziendali o scolastici nelle Impostazioni."],
    ["Review automatic sign-in and physical access to this PC before changing your sign-in routine.", "Controlla l'accesso automatico e chi può usare fisicamente il PC prima di cambiare il modo in cui accedi."],
    ["Review update service permissions with your administrator. Only a separately listed fix can be selected.", "Controlla i permessi dei servizi di aggiornamento con il tuo amministratore. Puoi selezionare solo una correzione elencata separatamente."],
    ["Ask your administrator to review antivirus service permissions.", "Chiedi al tuo amministratore di controllare i permessi del servizio antivirus."],
    ["Ask your administrator to review scheduled task service permissions.", "Chiedi al tuo amministratore di controllare i permessi del servizio di attività pianificate."],
    ["Ask your administrator to review Secblitz monitor service permissions.", "Chiedi al tuo amministratore di controllare i permessi del servizio di monitoraggio Secblitz."],
    ["Review saved changes before undoing them or making more changes.", "Controlla le modifiche salvate prima di annullarle o farne altre."],
    ["Less worry. More protection.", "Meno pensieri. Più protezione."],
    ["Your PC, checked.", "Il tuo PC, controllato."],
    ["Protection", "Protezione"],
    ["Status", "Stato"],
    ["What happens next", "Il prossimo passo"],
    ["Recommended fixes", "Correzioni consigliate"],
    ["Protected", "Protetto"],
    ["Needs your choice", "Scegli tu"],
    ["Good to go", "Tutto pronto"],
    ["Can fix", "Si può correggere"],
    ["Fixed", "Corretto"],
    ["Couldn't check", "Verifica non riuscita"],
    ["Managed elsewhere", "Gestito da altri"],
    ["Restart needed", "Serve un riavvio"],
    ["Checking", "Verifica in corso"],
    ["No checks were returned. Run a new check to review protection.", "Nessun risultato ricevuto. Ripeti la verifica per controllare la protezione."],
    ["Your saved changes were reviewed.", "Le modifiche salvate sono state esaminate."],
    ["Your changes are saved. Undo is available for recorded changes.", "Le modifiche sono salvate. Puoi annullare quelle registrate."],
    ["Live virus protection", "Protezione antivirus in tempo reale"],
    ["Suspicious app detection", "Rilevamento delle app sospette"],
    ["Downloaded file checks", "Controllo dei file scaricati"],
    ["Compressed file checks", "Controllo dei file compressi"],
    ["Work network firewall", "Firewall della rete di lavoro"],
    ["Home network firewall", "Firewall della rete di casa"],
    ["Public network firewall", "Firewall della rete pubblica"],
    ["Work network incoming connections", "Connessioni in entrata sulla rete di lavoro"],
    ["Home network incoming connections", "Connessioni in entrata sulla rete di casa"],
    ["Public network incoming connections", "Connessioni in entrata sulla rete pubblica"],
    ["Permission prompts", "Richieste di autorizzazione"],
    ["Administrator approval", "Consenso dell'amministratore"],
    ["App installation permissions", "Permessi per installare le app"],
    ["Account name privacy", "Riservatezza dei nomi degli account"],
    ["Remote sign-in safeguards", "Protezione degli accessi remoti"],
    ["Sign-in secret protection", "Protezione dei dati di accesso"],
    ["Update download protection", "Protezione del download degli aggiornamenti"],
    ["Windows Update tamper protection", "Windows Update al riparo dalle manomissioni"],
    ["Additional protection checks", "Altre verifiche di protezione"],
    ["Protection check", "Verifica della protezione"],
    ["Your security apps", "Le tue app di sicurezza"],
    ["Network protection", "Protezione della rete"],
    ["Virus protection", "Protezione antivirus"],
    ["Windows support", "Supporto di Windows"],
    ["Protection if your PC is lost", "Protezione in caso di smarrimento del PC"],
    ["Startup protection", "Protezione all'avvio"],
    ["Remote access", "Accesso remoto"],
    ["Older file sharing", "Vecchi sistemi di condivisione dei file"],
    ["Unsafe app and website warnings", "Avvisi su app e siti pericolosi"],
    ["Account sign-in safety", "Sicurezza dell'accesso agli account"],
    ["Core system protection", "Protezione del nucleo del sistema"],
    ["Who manages this PC", "Chi gestisce questo PC"],
    ["Automatic sign-in", "Accesso automatico"],
    ["Update download permissions", "Permessi per scaricare gli aggiornamenti"],
    ["Windows Update permissions", "Permessi di Windows Update"],
    ["Antivirus service permissions", "Permessi del servizio antivirus"],
    ["Scheduled task service permissions", "Permessi del servizio di attività pianificate"],
    ["Protection monitor permissions", "Permessi del monitor di protezione"],
    ["Saved changes", "Modifiche salvate"],
    ["readback differs from recorded target; pending transaction", "il valore riletto differisce da quello previsto registrato; transazione in sospeso"],
    ["readback differs from original; pending transaction", "il valore riletto differisce dall'originale; transazione in sospeso"],
    ["Unsupported service access mask requires manual review", "Una maschera di accesso al servizio non supportata richiede una verifica manuale"],
    ["DACL offset without DACL_PRESENT", "Offset della DACL senza DACL_PRESENT"],
    ["Deny, inherited or unsupported descriptor, ACE or access-mask semantics require manual evaluation; no dangerous supported ALLOW candidate found. No automatic repair.", "La semantica dei descrittori, delle ACE o delle maschere di accesso con negazione, ereditarietà o non supportati richiede una valutazione manuale; non è stata trovata alcuna ACE ALLOW pericolosa di tipo supportato. Nessuna correzione automatica."],
    ["Service permission eligibility requires Windows", "La verifica dei requisiti per modificare le autorizzazioni dei servizi richiede Windows"],
    ["Unknown service permission control id", "Identificativo del controllo delle autorizzazioni del servizio sconosciuto"],
    ["Invalid platform action arguments", "Argomenti dell'azione di piattaforma non validi"],
    ["Service permission gate was not acknowledged", "La verifica dei requisiti per le autorizzazioni del servizio non è stata confermata"],
    ["Computer Group Policy evidence: service permissions are assessment only", "Rilevati Criteri di gruppo del computer: le autorizzazioni dei servizi vengono solo valutate"],
    ["Applied computer policy settings: service permissions are assessment only", "Impostazioni dei criteri del computer applicate: le autorizzazioni dei servizi vengono solo valutate"],
    ["Local computer service policy artifacts: assessment only", "Rilevati elementi dei criteri locali dei servizi del computer: solo valutazione"],
    ["Invalid service permission gate request", "Richiesta di verifica dei requisiti per le autorizzazioni del servizio non valida"],
    ["Service permissions require the native wrapper", "Le autorizzazioni dei servizi richiedono l'adattatore nativo"],
    ["Repair dangerous BITS service permissions", "Correggi le autorizzazioni pericolose del servizio BITS"],
    ["Repair dangerous Windows Update service permissions", "Correggi le autorizzazioni pericolose del servizio Windows Update"],
    ["Repair dangerous ", "Correggi le autorizzazioni pericolose di "],
    [" service permissions", " (servizio)"],
    ["Remove dangerous explicit broad-principal service grants; preserve other ACE bytes and require exact-state rollback.", "Rimuovere le autorizzazioni esplicite pericolose concesse a gruppi estesi sul servizio; conservare gli altri byte delle ACE e richiedere il ripristino dello stato esatto."],
    ["Unknown service permission control", "Controllo delle autorizzazioni del servizio sconosciuto"],
    ["Service permission observation requires Windows", "La lettura delle autorizzazioni dei servizi richiede Windows"],
    ["Service permission repair requires Windows", "La correzione delle autorizzazioni dei servizi richiede Windows"],
    ["Service permission auditing requires Windows", "La verifica delle autorizzazioni dei servizi richiede Windows"],
    ["Service permission audit", "Verifica delle autorizzazioni dei servizi"],
    ["Truncated WORD", "WORD troncato"],
    ["Truncated DWORD", "DWORD troncato"],
    ["Invalid SID", "SID non valido"],
    ["Invalid SID size", "Dimensione del SID non valida"],
    ["Service owner is not SYSTEM, Administrators or TrustedInstaller", "Il proprietario del servizio non è SYSTEM, Administrators né TrustedInstaller"],
    ["Service owner, group or descriptor flags changed", "Il proprietario, il gruppo o i flag del descrittore del servizio sono cambiati"],
    ["Service DACL drift or non-repair transition", "DACL del servizio modificata o transizione non riconducibile alla correzione"],
    ["Invalid descriptor header", "Intestazione del descrittore non valida"],
    ["Unsupported descriptor flags", "Flag del descrittore non supportati"],
    ["SACL snapshots are forbidden", "Le istantanee SACL non sono consentite"],
    ["Invalid SID offset", "Offset del SID non valido"],
    ["Truncated SID", "SID troncato"],
    ["Invalid DACL offset", "Offset della DACL non valido"],
    ["Truncated DACL", "DACL troncata"],
    ["Overlapping descriptor sections", "Sezioni del descrittore sovrapposte"],
    ["Service DACL state must be a string", "Lo stato della DACL del servizio deve essere una stringa"],
    ["Service DACL state exceeds limit", "Lo stato della DACL del servizio supera il limite"],
    ["Invalid service DACL state version", "Versione dello stato della DACL del servizio non valida"],
    ["Noncanonical DACL hex", "Rappresentazione esadecimale della DACL non canonica"],
    ["Noncanonical DACL descriptor offsets or trailing bytes", "Offset del descrittore DACL non canonici o byte aggiuntivi in coda"],
    ["NULL or absent DACL requires manual review", "Una DACL NULL o assente richiede una verifica manuale"],
    ["Deny, inherited, flagged or unsupported ACE requires manual review", "Una ACE di negazione, ereditata, con flag o non supportata richiede una verifica manuale"],
    ["Invalid ACL header", "Intestazione ACL non valida"],
    ["Invalid ACL size", "Dimensione ACL non valida"],
    ["Truncated ACE", "ACE troncata"],
    ["Invalid ACE size", "Dimensione ACE non valida"],
    ["Truncated ALLOW/DENY ACE", "ACE ALLOW/DENY troncata"],
    ["Invalid SID header", "Intestazione del SID non valida"],
    ["Invalid SID length", "Lunghezza del SID non valida"],
    ["Invalid security descriptor header", "Intestazione del descrittore di sicurezza non valida"],
    ["Descriptor is not self-relative", "Il descrittore non è autorelativo"],
    ["DACL outside descriptor", "DACL esterna al descrittore"],
    ["Truncated ACL", "ACL troncata"],
    ["Truncated ACE header", "Intestazione ACE troncata"],
    ["Missing ACE SID", "SID della ACE mancante"],
    ["ALLOW mask", "maschera ALLOW"],
    ["risky bits", "bit rischiosi"],
    ["Invalid service descriptor size", "Dimensione del descrittore del servizio non valida"],
    ["Config string outside buffer", "Stringa di configurazione esterna al buffer"],
    ["Invalid config string offset", "Offset della stringa di configurazione non valido"],
    ["Unterminated service config string", "Stringa di configurazione del servizio priva del carattere di terminazione"],
    ["Invalid service config size", "Dimensione della configurazione del servizio non valida"],
    ["Unexpected built-in service type", "Tipo del servizio integrato inatteso"],
    ["Unexpected built-in service account", "Account del servizio integrato inatteso"],
    ["Cannot resolve Windows system directory", "Impossibile individuare la cartella di sistema di Windows"],
    ["Invalid system directory", "Cartella di sistema non valida"],
    ["Unexpected built-in service executable configuration", "Configurazione dell'eseguibile del servizio integrato inattesa"],
    ["Service host is not a regular non-reparse file", "Il processo host del servizio non corrisponde a un file regolare privo di punti di analisi"],
    ["Invalid service host owner", "Proprietario del file host del servizio non valido"],
    ["Invalid service host SID size", "Dimensione del SID del file host del servizio non valida"],
    ["Eligible service permission repair", "Correzione delle autorizzazioni del servizio consentita"],
    ["Service permissions preserved: ", "Autorizzazioni del servizio conservate: "],
    ["Service descriptor changed before write", "Il descrittore del servizio è cambiato prima della scrittura"],
    ["Service DACL exact readback mismatch; mutation outcome requires review", "La rilettura esatta della DACL del servizio non corrisponde al valore atteso; verificare l'esito della modifica"],
    ["Service permissions: ", "Autorizzazioni del servizio: "],
    ["Service is not installed; no DACL assessed.", "Il servizio non è installato; nessuna DACL valutata."],
    ["Invalid security descriptor length", "Lunghezza del descrittore di sicurezza non valida"],
    ["review", "da verificare"],
    ["Absent or NULL DACL permits unrestricted access. Administrator investigation required; no automatic repair.", "Una DACL assente o NULL consente l'accesso senza restrizioni. È necessaria un'indagine da parte dell'amministratore; nessuna correzione automatica."],
    ["Candidate dangerous broad-principal grants: ", "Possibili autorizzazioni pericolose concesse a gruppi estesi: "],
    ["Deny, inherited or unsupported ACE semantics require manual evaluation. ", "La semantica delle ACE di negazione, ereditate o non supportate richiede una valutazione manuale. "],
    ["This is an ACE scan, not effective access or proof of exploitability. ", "Questa è un'analisi delle ACE, non una valutazione dell'accesso effettivo né una prova che la vulnerabilità sia sfruttabile. "],
    ["Consult the fixed service repair control for gated eligibility.", "Consultare il controllo di correzione specifico del servizio per verificarne i requisiti."],
    ["Review with the service owner; no automatic repair for this service.", "Verificare con il responsabile del servizio; nessuna correzione automatica per questo servizio."],
    ["Deny, inherited or unsupported ACE semantics require manual evaluation; no dangerous supported ALLOW candidate found. No automatic repair.", "La semantica delle ACE di negazione, ereditate o non supportate richiede una valutazione manuale; non è stata trovata alcuna ACE ALLOW pericolosa di tipo supportato. Nessuna correzione automatica."],
    ["No dangerous ALLOW bits found for Everyone, Authenticated Users or Builtin Users in this DACL. Limited scan: other principals, ownership and executable paths were not assessed.", "Nessun bit ALLOW pericoloso trovato per Everyone, Authenticated Users o Builtin Users in questa DACL. Analisi limitata: gli altri soggetti, la proprietà e i percorsi degli eseguibili non sono stati valutati."],
    ["Cannot connect to local SCM: ", "Impossibile connettersi a SCM locale: "],
    ["DACL could not be assessed: ", "Impossibile valutare la DACL: "],
    ["No change made.", "Nessuna modifica effettuata."],
    ["Disable always-elevated MSI installation", "Disattiva l'installazione MSI con privilegi sempre elevati"],
    ["Restrict anonymous SAM enumeration", "Limita l'enumerazione anonima degli account SAM"],
    ["Limit blank-password accounts to console logon", "Limita gli account senza password all'accesso dalla console"],
    ["Disable WDigest plaintext credential caching", "Disattiva la memorizzazione delle credenziali WDigest in chiaro"],
    ["Repair only machine AlwaysInstallElevated=1. The machine setting breaks the vulnerable machine/user conjunction; preserve HKCU, absent values and normal administrator-authorized installs.", "Correggere solo AlwaysInstallElevated=1 a livello di computer. L'impostazione del computer interrompe la combinazione vulnerabile tra computer e utente; conservare HKCU, i valori assenti e le normali installazioni autorizzate dall'amministratore."],
    ["Repair only RestrictAnonymousSAM=0. Require authentication for account enumeration; legacy anonymous enumeration workflows may be affected. Preserve absent values and other LSA settings.", "Correggere solo RestrictAnonymousSAM=0. Richiedere l'autenticazione per enumerare gli account; le procedure obsolete che usano l'enumerazione anonima potrebbero risentirne. Conservare i valori assenti e le altre impostazioni LSA."],
    ["Repair only LimitBlankPasswordUse=0. Block remote logons using blank local passwords while preserving physical console logon. Preserve absent values; no passwords are inspected or changed.", "Correggere solo LimitBlankPasswordUse=0. Bloccare gli accessi remoti con password locali vuote, mantenendo l'accesso dalla console fisica. Conservare i valori assenti; nessuna password viene esaminata o modificata."],
    ["Repair only UseLogonCredential=1. Preserve absent values (safe on supported Windows). Readback verifies stored configuration, not running LSASS; restart/sign-out may be needed for existing sessions. Legacy Digest SSO may require credentials.", "Correggere solo UseLogonCredential=1. Conservare i valori assenti (sicuri nelle versioni di Windows supportate). La rilettura verifica la configurazione salvata, non LSASS in esecuzione; per le sessioni esistenti potrebbe servire un riavvio o una disconnessione. Il vecchio accesso SSO Digest potrebbe richiedere le credenziali."],
    ["Invalid registry DWORD", "Valore DWORD del Registro di sistema non valido"],
    ["Other machine Installer policy is configured: assessment only", "È configurato un altro criterio di Installer a livello di computer: solo valutazione"],
    ["Invalid binary registry state", "Stato binario del Registro di sistema non valido"],
    ["Invalid binary registry DWORD", "Valore DWORD binario del Registro di sistema non valido"],
    ["Unknown privilege control", "Controllo dei privilegi sconosciuto"],
    ["Privilege preference is not a DWORD", "L'impostazione dei privilegi non è un DWORD"],
    ["Privilege repair requires an explicitly unsafe current setting", "La correzione dei privilegi richiede un'impostazione corrente esplicitamente non sicura"],
    ["Privilege restore requires the current target setting; preference changed before restore", "Il ripristino dei privilegi richiede che sia ancora presente il valore previsto; l'impostazione è cambiata prima del ripristino"],
    ["Privilege registry readback did not match; mutation outcome requires review", "La rilettura dei privilegi nel Registro di sistema non corrisponde al valore atteso; verificare l'esito della modifica"],
    ["AutoAdminLogon is not a string", "AutoAdminLogon non è una stringa"],
    ["AutoAdminLogon has an unknown configuration", "AutoAdminLogon presenta una configurazione sconosciuta"],
    ["Automatic logon", "Accesso automatico"],
    ["Preserving absent or already-safe machine preference", "L'impostazione del computer assente o già sicura viene conservata"],
    ["AutoAdminLogon enabled=", "AutoAdminLogon attivo="],
    ["Winlogon DefaultPassword value present=", "Valore DefaultPassword di Winlogon presente="],
    ["Presence only: no password data is read. LSA-secret autologon storage is not inspected. Review physical access and credential exposure; automatic logon is preserved to avoid disrupting kiosk or sign-in workflows.", "Solo presenza: nessun dato delle password viene letto. Le credenziali di accesso automatico archiviate nei segreti LSA non vengono esaminate. Verificare l'accesso fisico e l'esposizione delle credenziali; l'accesso automatico viene conservato per non interrompere il funzionamento dei chioschi o le procedure di accesso."],
    ["Relevant policy is configured or its authority is unknown: assessment only", "È configurato un criterio pertinente oppure non è nota l'autorità che lo gestisce: solo valutazione"],
    ["Group Policy authority is unknown: assessment only", "L'autorità che gestisce i Criteri di gruppo non è nota: solo valutazione"],
    ["Relevant resultant Group Policy: assessment only", "Criteri di gruppo risultanti pertinenti: solo valutazione"],
    ["Firewall preference/effective readback did not match; mutation outcome requires review", "La rilettura dell'impostazione o dello stato effettivo del firewall non corrisponde al valore atteso; verificare l'esito della modifica"],
    ["No device-management registration or UAC policy authority found by the available probes. Each control repeats scoped policy and capability checks before mutation.", "Le verifiche disponibili non hanno rilevato registrazioni alla gestione del dispositivo né autorità responsabili dei criteri UAC. Prima di ogni modifica, ciascun controllo ripete le verifiche dei criteri pertinenti e delle funzionalità disponibili."],
    ["requires a JSON boolean", "richiede un valore booleano JSON"],
    ["MDM registration state is unknown", "Lo stato di registrazione MDM non è noto"],
    ["API result=", "risultato API="],
    ["Device is registered with MDM: assessment only", "Il dispositivo è registrato alla gestione MDM: solo valutazione"],
    ["Probe returned an invalid finding", "La verifica ha restituito un risultato non valido"],
    ["Assessment unavailable", "Valutazione non disponibile"],
    ["Findings could not be collected", "Impossibile raccogliere i risultati delle verifiche"],
    [" has incomplete apply or rollback; use revert to resolve its recorded preferences before applying again.", " presenta un'applicazione o un ripristino incompleto; usare revert per risolvere le impostazioni registrate prima di applicare altre modifiche."],
    ["Enable the core Defender preference only on an unmanaged device with no competing antivirus. Preserve exclusions and other preferences.", "Attivare l'impostazione di base di Defender solo su un dispositivo non gestito e senza altri antivirus. Conservare le esclusioni e le altre impostazioni."],
    ["Preserve all firewall rules and outbound policy. Unmanaged devices only.", "Conservare tutte le regole del firewall e i criteri per il traffico in uscita. Solo dispositivi non gestiti."],
    ["Repair an explicitly disabled EnableLUA value; preserve absent or already enabled settings.", "Correggere un valore EnableLUA esplicitamente disattivato; conservare le impostazioni assenti o già attive."],
    ["Repair consent mode 0 to Windows default 5. Preserve every nonzero mode.", "Portare la modalità di consenso da 0 al valore predefinito di Windows, 5. Conservare qualsiasi modalità diversa da zero."],
    ["A fixed local drive is required", "È necessaria un'unità locale fissa"],
    ["Secblitz read-only security monitor", "Monitor di sicurezza Secblitz in sola lettura"],
    ["Read-only security observations every 15 minutes. No automatic remediation; latest report in Program Files/Secblitz/Monitor.", "Verifiche di sicurezza in sola lettura ogni 15 minuti. Nessuna correzione automatica; ultimo rapporto in Program Files/Secblitz/Monitor."],
    ["Not applicable", "Non applicabile"],
    ["Service exit code", "Codice di uscita del servizio"],
    ["Checkpoint", "Punto di controllo"],
    ["Wait hint (ms)", "Attesa stimata (ms)"],
    ["Expected an absolute local drive path", "È richiesto un percorso assoluto su un'unità locale"],
    ["Invalid Windows path characters", "Caratteri non validi nel percorso Windows"],
    ["Ambiguous Windows path component", "Componente ambiguo del percorso Windows"],
    ["Reserved Windows device name", "Nome di dispositivo riservato da Windows"],
    ["Embedded NUL", "Carattere NUL incorporato"],
    ["Security descriptor", "Descrittore di sicurezza"],
    ["Non-Unicode Windows path", "Percorso Windows non Unicode"],
    ["Cannot resolve Program Files", "Impossibile individuare Program Files"],
    ["File information", "Informazioni sul file"],
    ["Reparse point rejected", "Punto di analisi rifiutato"],
    ["Wrong object type", "Tipo di oggetto errato"],
    ["Hard-linked file rejected", "File con collegamenti fisici rifiutato"],
    ["Cannot inspect file ACL", "Impossibile esaminare l'ACL del file"],
    ["Cannot inspect owner", "Impossibile verificare il proprietario"],
    ["Cannot inspect DACL control", "Impossibile esaminare i flag di controllo della DACL"],
    ["Cannot inspect DACL", "Impossibile esaminare la DACL"],
    ["Untrusted file owner", "Proprietario del file non attendibile"],
    ["Missing/invalid DACL", "DACL assente o non valida"],
    ["Unprotected DACL", "DACL non protetta"],
    ["Cannot inspect ACE", "Impossibile esaminare l'ACE"],
    ["Short ACE", "ACE troppo corta"],
    ["Unsupported ACL entry", "Voce ACL non supportata"],
    ["Unexpected ACE flags/type", "Flag o tipo di ACE inattesi"],
    ["Invalid trustee SID", "SID del destinatario delle autorizzazioni non valido"],
    ["Unexpected protected-object trustee", "Destinatario delle autorizzazioni inatteso per l'oggetto protetto"],
    ["Unexpected protected-object rights", "Autorizzazioni inattese per l'oggetto protetto"],
    ["Missing ACL propagation", "Propagazione dell'ACL assente"],
    ["Writable/untrusted ancestor DACL", "DACL di una cartella antenata modificabile o non attendibile"],
    ["Missing protected-object trustees", "Destinatari delle autorizzazioni mancanti per l'oggetto protetto"],
    ["Untrusted ancestor", "Cartella antenata non attendibile"],
    ["Create directory failed", "Creazione della cartella non riuscita"],
    ["Service installation requires Administrator elevation", "L'installazione del servizio richiede privilegi di amministratore"],
    ["The monitor requires Windows x64", "Il monitor richiede Windows x64"],
    ["SecblitzMonitor already exists; it was not changed", "SecblitzMonitor esiste già; non è stato modificato"],
    ["Invalid installer executable", "Eseguibile di installazione non valido"],
    ["Destination binary exists and is not this installer; refusing overwrite", "L'eseguibile di destinazione esiste e non corrisponde a questo programma di installazione; sovrascrittura rifiutata"],
    ["Cannot secure service configuration", "Impossibile proteggere la configurazione del servizio"],
    ["Cannot set service command", "Impossibile impostare il comando del servizio"],
    ["Cannot restrict service privileges", "Impossibile limitare i privilegi del servizio"],
    ["Cannot enable automatic startup", "Impossibile attivare l'avvio automatico"],
    ["Install failed", "Installazione non riuscita"],
    ["rollback could not remove registration; files retained", "il ripristino non ha potuto rimuovere la registrazione; file conservati"],
    ["Installation rolled back; cleanup failures", "Installazione annullata; errori durante la pulizia"],
    ["Service removal requires Administrator elevation", "La rimozione del servizio richiede privilegi di amministratore"],
    ["Unexpected service configuration; refusing to delete", "Configurazione del servizio inattesa; eliminazione rifiutata"],
    ["Stop SecblitzMonitor through SCM before uninstalling", "Arrestare SecblitzMonitor tramite SCM prima di disinstallarlo"],
    ["Monitor worker panicked", "Il processo di monitoraggio si è interrotto in modo imprevisto"],
    ["Monitor report exceeded limit", "Il rapporto del monitor ha superato il limite"],
    ["Report limit", "Limite del rapporto"],
    ["Cannot inspect journal handle", "Impossibile esaminare l'handle del registro delle modifiche"],
    ["Cannot inspect journal identity", "Impossibile verificare l'identità del registro delle modifiche"],
    ["Non-UTF8 journal filename", "Nome del file del registro delle modifiche non UTF-8"],
    ["Journal schema, machine, or transaction identity mismatch", "Schema, computer o identità della transazione non corrispondenti nel registro delle modifiche"],
    ["The protected journal directory is only available on Windows", "La cartella protetta del registro delle modifiche è disponibile solo su Windows"],
    ["Embedded NUL in Windows string", "Carattere NUL incorporato nella stringa Windows"],
    ["Cannot resolve trusted Windows directory", "Impossibile individuare la cartella Windows attendibile"],
    ["Windows directory is not absolute", "Il percorso della cartella Windows non è assoluto"],
    ["Invalid elevation information", "Informazioni non valide sull'elevazione dei privilegi"],
    ["Elevation was cancelled or failed", "Elevazione dei privilegi annullata o non riuscita"],
    ["ShellExecute code", "Codice ShellExecute"],
    ["Cannot create bounded process job", "Impossibile creare un oggetto job con limiti per il processo"],
    ["Secblitz supports Windows x64 only", "Secblitz supporta solo Windows x64"],
    ["Unknown platform action", "Azione di piattaforma sconosciuta"],
    ["Missing control id", "Identificativo del controllo mancante"],
    ["Start inbox Windows PowerShell", "Avvio di Windows PowerShell incluso nel sistema"],
    ["Assign PowerShell job", "Assegnazione del job di PowerShell"],
    ["Missing stdin", "stdin assente"],
    ["Missing stdout", "stdout assente"],
    ["Missing stderr", "stderr assente"],
    ["PowerShell timed out; mutation outcome may be unknown", "Tempo scaduto per PowerShell; l'esito della modifica potrebbe essere sconosciuto"],
    ["PowerShell output timeout/disconnect; mutation outcome may be unknown", "Tempo scaduto o disconnessione dell'output di PowerShell; l'esito della modifica potrebbe essere sconosciuto"],
    ["PowerShell exceeded 2 MiB output limit; mutation outcome may be unknown", "PowerShell ha superato il limite di output di 2 MiB; l'esito della modifica potrebbe essere sconosciuto"],
    ["PowerShell exit timed out; mutation outcome may be unknown", "Tempo scaduto in attesa della chiusura di PowerShell; l'esito della modifica potrebbe essere sconosciuto"],
    ["PowerShell failed", "Esecuzione di PowerShell non riuscita"],
    ["PowerShell input writer failed", "Scrittura dell'input di PowerShell non riuscita"],
    ["Invalid PowerShell response (no result assumed)", "Risposta di PowerShell non valida (nessun risultato dato per acquisito)"],
    ["Inbox Windows PowerShell is unavailable", "Windows PowerShell incluso nel sistema non è disponibile"],
    ["Invalid Windows machine identity", "Identità del computer Windows non valida"],
    ["Administrator elevation is required", "Sono richiesti privilegi di amministratore"],
    ["Write was not acknowledged", "La scrittura non è stata confermata"],
    ["Preference readback did not match; mutation outcome requires review", "La rilettura dell'impostazione non corrisponde al valore atteso; verificare l'esito della modifica"],
    ["Cannot open journal path safely", "Impossibile aprire in sicurezza il percorso del registro delle modifiche"],
    ["Journal path contains a reparse point", "Il percorso del registro delle modifiche contiene un punto di analisi"],
    ["Journal file has multiple hard links", "Il file del registro delle modifiche ha più collegamenti fisici"],
    ["Cannot query journal security", "Impossibile leggere le informazioni di sicurezza del registro delle modifiche"],
    ["Windows error", "Errore di Windows"],
    ["Journal path owner is not SYSTEM or Administrators", "Il proprietario del percorso del registro delle modifiche non è SYSTEM né Administrators"],
    ["Journal DACL is missing or invalid", "La DACL del registro delle modifiche è assente o non valida"],
    ["Journal DACL is not present", "La DACL del registro delle modifiche non è presente"],
    ["Journal root DACL permits inheritance", "La DACL della cartella radice del registro delle modifiche consente l'ereditarietà"],
    ["Unexpected journal ACE type", "Tipo di ACE inatteso nel registro delle modifiche"],
    ["Unexpected journal ACE flags", "Flag ACE inattesi nel registro delle modifiche"],
    ["Journal directory does not propagate its restricted DACL", "La cartella del registro delle modifiche non propaga la propria DACL restrittiva"],
    ["Unexpected journal access mask", "Maschera di accesso inattesa per il registro delle modifiche"],
    ["Invalid journal trustee SID", "SID del destinatario delle autorizzazioni del registro delle modifiche non valido"],
    ["Untrusted journal trustee", "Destinatario delle autorizzazioni del registro delle modifiche non attendibile"],
    ["Journal must grant full control to SYSTEM and Administrators", "Il registro delle modifiche deve concedere il controllo completo a SYSTEM e Administrators"],
    ["Cannot resolve ProgramData known folder", "Impossibile individuare la cartella nota ProgramData"],
    ["Invalid known-folder path", "Percorso della cartella nota non valido"],
    ["Journal directory exceeds inspection limits", "La cartella del registro delle modifiche supera i limiti di verifica"],
    ["Inspect", "Esamina"],
    ["Untrusted journal entry", "Voce del registro delle modifiche non attendibile"],
    ["Too many journal entries", "Troppe voci nel registro delle modifiche"],
    ["Journal lock poisoned", "Blocco del registro delle modifiche invalidato da un errore precedente"],
    ["Protected journal access requires Administrator elevation", "L'accesso al registro protetto delle modifiche richiede privilegi di amministratore"],
    ["ProgramData must be on a local drive", "ProgramData deve trovarsi su un'unità locale"],
    ["ProgramData is not absolute", "Il percorso di ProgramData non è assoluto"],
    ["Noncanonical ProgramData path", "Percorso di ProgramData non canonico"],
    ["Cannot inspect volume root", "Impossibile esaminare la radice del volume"],
    ["Invalid volume root", "Radice del volume non valida"],
    ["Journal requires a fixed local drive", "Il registro delle modifiche richiede un'unità locale fissa"],
    ["ProgramData ancestor is not a directory", "Un elemento antenato di ProgramData non è una cartella"],
    ["Cannot construct journal security descriptor", "Impossibile creare il descrittore di sicurezza del registro delle modifiche"],
    ["Cannot create protected journal directory", "Impossibile creare la cartella protetta del registro delle modifiche"],
    ["Journal root is not a directory", "La radice del registro delle modifiche non è una cartella"],
    ["ProgramData changed during this process", "ProgramData è cambiato durante questo processo"],
    ["Unknown control id", "Identificativo del controllo sconosciuto"],
    ["Unknown control", "Controllo sconosciuto"],
    ["Unknown operation", "Operazione sconosciuta"],
    ["Invalid MachineGuid", "MachineGuid non valido"],
    ["Defender tamper state changed or is unknown; mutation outcome requires review", "Lo stato della protezione antimanomissione di Defender è cambiato o non è noto; verificare l'esito della modifica"],
    ["Defender became unavailable or passive; mutation outcome requires review", "Defender è diventato non disponibile o è passato in modalità passiva; verificare l'esito della modifica"],
    ["Defender preference/runtime readback did not match; mutation outcome requires review", "La rilettura dell'impostazione o dello stato di esecuzione di Defender non corrisponde al valore atteso; verificare l'esito della modifica"],
    ["UAC restore requires the current target setting; preference changed before restore", "Il ripristino di UAC richiede che sia ancora presente il valore previsto; l'impostazione è cambiata prima del ripristino"],
    ["Invalid boolean preference for", "Impostazione booleana non valida per"],
    ["Invalid inbound preference", "Impostazione del traffico in entrata non valida"],
    ["Invalid inbound action", "Azione per il traffico in entrata non valida"],
    ["Invalid UAC preference", "Impostazione UAC non valida"],
    ["Invalid UAC fields", "Campi UAC non validi"],
    ["Absent UAC preference must have null value", "Un'impostazione UAC assente deve avere valore null"],
    ["Invalid UAC DWORD", "Valore DWORD di UAC non valido"],
    ["Invalid UAC presence flag", "Indicatore di presenza UAC non valido"],
    ["Registry state must contain exactly present and value", "Lo stato del Registro di sistema deve contenere esattamente present e value"],
    ["Absent registry value must be null", "Un valore assente nel Registro di sistema deve essere null"],
    ["Journal links are forbidden", "I collegamenti nel registro delle modifiche non sono consentiti"],
    ["Unexpected journal file type", "Tipo di file inatteso per il registro delle modifiche"],
    ["Journal hard links are forbidden", "I collegamenti fisici nel registro delle modifiche non sono consentiti"],
    ["Journal reparse points are forbidden", "I punti di analisi nel registro delle modifiche non sono consentiti"],
    ["Cannot inspect journal file", "Impossibile esaminare il file del registro delle modifiche"],
    ["Open journal", "Apertura del registro delle modifiche"],
    ["Journal file identity changed", "L'identità del file del registro delle modifiche è cambiata"],
    ["Engine requires the protected platform journal directory", "Il motore richiede la cartella protetta del registro delle modifiche della piattaforma"],
    ["Duplicate backend control", "Controllo duplicato nel motore"],
    ["Backend target differs from compiled target", "Il valore previsto dal motore differisce da quello definito in compilazione"],
    ["Invalid machine identity", "Identità del computer non valida"],
    ["Journal storage failed; reopen the engine after resolving storage failure", "Salvataggio del registro delle modifiche non riuscito; riaprire il motore dopo aver risolto il problema di archiviazione"],
    ["Another Secblitz operation holds the journal lock", "Un'altra operazione di Secblitz detiene il blocco del registro delle modifiche"],
    ["Journal control is not supported by this backend", "Il controllo presente nel registro delle modifiche non è supportato da questo motore"],
    ["Too many journal transactions", "Troppe transazioni nel registro delle modifiche"],
    ["Unexpected journal entry", "Voce inattesa nel registro delle modifiche"],
    ["Invalid journal filename", "Nome del file del registro delle modifiche non valido"],
    ["Invalid journal sequence", "Sequenza del registro delle modifiche non valida"],
    ["Invalid transaction UUID", "UUID della transazione non valido"],
    ["Journal exceeds size limit", "Il registro delle modifiche supera il limite di dimensione"],
    ["Empty, oversized, or truncated journal", "Registro delle modifiche vuoto, troppo grande o troncato"],
    ["Invalid journal record size", "Dimensione non valida della voce del registro delle modifiche"],
    ["Invalid journal record", "Voce non valida nel registro delle modifiche"],
    ["Missing header", "Intestazione mancante"],
    ["Journal must start with a header", "Il registro delle modifiche deve iniziare con un'intestazione"],
    ["Records after transaction completion", "Voci presenti dopo il completamento della transazione"],
    ["Redundant before image", "Copia dello stato precedente ridondante"],
    ["Invalid prepare ordering or duplicate before image", "Ordine di preparazione non valido o copia dello stato precedente duplicata"],
    ["Apply after seal/revert", "Applicazione dopo la chiusura o l'annullamento"],
    ["Apply without prepare", "Applicazione senza preparazione"],
    ["Invalid apply result", "Esito dell'applicazione non valido"],
    ["Invalid seal", "Chiusura non valida"],
    ["Duplicate revert start", "Avvio dell'annullamento duplicato"],
    ["Restore before revert start", "Ripristino prima dell'avvio dell'annullamento"],
    ["Restore without before image", "Ripristino senza copia dello stato precedente"],
    ["Restore after completion", "Ripristino dopo il completamento"],
    ["Restore result before revert start", "Esito del ripristino precedente all'avvio dell'annullamento"],
    ["Result without before image", "Esito senza copia dello stato precedente"],
    ["Restore result without intent", "Esito del ripristino senza intenzione registrata"],
    ["Premature revert completion", "Completamento prematuro dell'annullamento"],
    ["Duplicate journal header", "Intestazione del registro delle modifiche duplicata"],
    ["Duplicate transaction sequence", "Numero di sequenza della transazione duplicato"],
    ["Journal length changed since validation", "La lunghezza del registro delle modifiche è cambiata dopo la convalida"],
    ["Journal record exceeds limit", "La voce del registro delle modifiche supera il limite"],
    ["Journal is full", "Il registro delle modifiche è pieno"],
    ["Transaction sequence exhausted", "Numeri di sequenza delle transazioni esauriti"],
    ["changed or became ineligible after prepare; revert transaction", "è cambiato o non è più idoneo dopo la preparazione; annullare la transazione"],
    ["has unknown outcome; pending transaction", "ha un esito sconosciuto; transazione in sospeso"],
    ["retained", "conservata"],
    ["Apply", "Applica"],
    ["Restore", "Ripristina"],
    ["Installed SecblitzMonitor as LocalService; it has not been started. Binary and reports are retained on uninstall.", "SecblitzMonitor installato come LocalService; non è stato avviato. L'eseguibile e i rapporti vengono conservati alla disinstallazione."],
    ["SecblitzMonitor registration is removed or was already absent. Binary, reports and journals are preserved.", "La registrazione di SecblitzMonitor è stata rimossa o era già assente. L'eseguibile, i rapporti e i registri delle modifiche sono conservati."],
    ["Service diagnostic (native codes and values)", "Diagnostica del servizio (codici e valori nativi)"],
    ["Password generation failed", "Generazione della password non riuscita"],
    ["Password output failed", "Visualizzazione della password non riuscita"],
    ["not installed", "non installato"],
    ["Stopped", "Arrestato"],
    ["Running", "In esecuzione"],
    ["StartPending", "Avvio in corso"],
    ["StopPending", "Arresto in corso"],
    ["ContinuePending", "Ripresa in corso"],
    ["PausePending", "Sospensione in corso"],
    ["Paused", "Sospeso"],
    ["exit=", "uscita="],
    ["checkpoint=", "punto di controllo="],
    ["wait=", "attesa="],
    ["App Installer package path is not absolute", "Il percorso del pacchetto App Installer non è assoluto"],
    ["Resolve App Installer package directory", "Individuazione della cartella del pacchetto App Installer"],
    ["Resolve packaged winget.exe", "Individuazione di winget.exe nel pacchetto"],
    ["Packaged winget.exe escapes its registered package directory or is not a file", "winget.exe si trova fuori dalla cartella registrata del pacchetto oppure non è un file"],
    ["Invalid TokenUser buffer size", "Dimensione del buffer TokenUser non valida"],
    ["Open desktop shell process", "Apertura del processo della shell del desktop"],
    ["SHGetKnownFolderPath failed", "Chiamata a SHGetKnownFolderPath non riuscita"],
    ["SHGetKnownFolderPath returned a null path", "SHGetKnownFolderPath ha restituito un percorso nullo"],
    ["Known folder is not an absolute path", "Il percorso della cartella nota non è assoluto"],
    ["Check existing Bitwarden", "Verifica dell'installazione esistente di Bitwarden"],
    ["OpenPackageInfoByFullName failed", "Chiamata a OpenPackageInfoByFullName non riuscita"],
    ["GetPackageInfo sizing failed", "Calcolo della dimensione con GetPackageInfo non riuscito"],
    ["GetPackageInfo failed", "Chiamata a GetPackageInfo non riuscita"],
    ["App Installer is a developer-mode registration; use the packaged Microsoft Store installation", "App Installer è registrato in modalità sviluppatore; usare il pacchetto distribuito tramite Microsoft Store"],
    ["Microsoft App Installer is not available for this user", "Microsoft App Installer non è disponibile per questo utente"],
    ["install or repair App Installer through Microsoft Store", "installare o riparare App Installer tramite Microsoft Store"],
    ["GetPackagesByPackageFamily failed", "Chiamata a GetPackagesByPackageFamily non riuscita"],
    ["GetPackagePathByFullName sizing failed", "Calcolo della dimensione con GetPackagePathByFullName non riuscito"],
    ["GetPackagePathByFullName failed", "Chiamata a GetPackagePathByFullName non riuscita"],
    ["Unterminated package path", "Percorso del pacchetto privo del carattere di terminazione"],
    ["Check registered App Installer executable", "Verifica dell'eseguibile registrato di App Installer"],
    ["Expected one registered App Installer executable, found", "Era previsto un solo eseguibile registrato di App Installer; trovati"],
    ["repair App Installer for this user", "riparare App Installer per questo utente"],
    ["Invalid packaged executable path", "Percorso dell'eseguibile nel pacchetto non valido"],
    ["Missing package directory", "Cartella del pacchetto mancante"],
    ["Protect pipe reader from inheritance", "Protezione dell'handle di lettura della pipe dall'ereditarietà"],
    ["Open null input", "Apertura dell'input nullo"],
    ["Set null input inheritance", "Impostazione dell'ereditarietà dell'input nullo"],
    ["Start packaged WinGet", "Avvio di WinGet dal pacchetto"],
    ["Assign WinGet to timeout job", "Assegnazione di WinGet al job con limite di tempo"],
    ["Resume WinGet failed", "Ripresa di WinGet non riuscita"],
    ["exceeded the ten-minute deadline; its job was terminated; check installation state before retrying", "ha superato il limite di dieci minuti; il job è stato terminato; verificare lo stato dell'installazione prima di riprovare"],
    ["Read WinGet output pipe", "Lettura della pipe di output di WinGet"],
    ["Read WinGet output", "Lettura dell'output di WinGet"],
    ["Source", "Origine"],
    ["List", "Elenco"],
    ["Install", "Installazione"],
    ["exit", "uscita"],
    ["count", "conteggio"],
    ["Run tools bitwarden --yes from the original user's non-elevated desktop, not an administrator terminal", "Eseguire tools bitwarden --yes dal desktop dell'utente originale senza privilegi elevati, non da un terminale amministratore"],
    ["No desktop shell: Bitwarden installation cannot run as a service or background account", "Shell del desktop assente: l'installazione di Bitwarden non può essere eseguita come servizio o da un account in background"],
    ["Cannot identify desktop shell user", "Impossibile identificare l'utente della shell del desktop"],
    ["Current account differs from the desktop user; run tools bitwarden --yes as that user without elevation", "L'account corrente è diverso dall'utente del desktop; eseguire tools bitwarden --yes come tale utente senza privilegi elevati"],
    ["WinGet Bitwarden detection failed", "Rilevamento di Bitwarden tramite WinGet non riuscito"],
    ["WinGet source export did not return a single JSON source", "L'esportazione delle origini di WinGet non ha restituito una sola origine JSON"],
    ["WinGet repository verification failed: unexpected", "Verifica del repository WinGet non riuscita: valore inatteso"],
    ["Bitwarden operation exceeded its ten-minute deadline", "L'operazione di Bitwarden ha superato il limite di dieci minuti"],
    ["WinGet source export failed", "Esportazione dell'origine WinGet non riuscita"],
    ["Determine existing installation; no install was attempted", "Verifica dell'installazione esistente; nessuna installazione tentata"],
    ["WinGet Bitwarden installation failed", "Installazione di Bitwarden tramite WinGet non riuscita"],
    ["installer hash verification was not bypassed", "la verifica dell'hash del programma di installazione non è stata aggirata"],
    ["WinGet reported success but Bitwarden desktop was not found afterward", "WinGet ha segnalato il completamento, ma in seguito l'applicazione desktop Bitwarden non è stata trovata"],
    ["Invalid elevation arguments", "Argomenti per l'elevazione dei privilegi non validi"],
    ["Elevation returned no process handle", "L'elevazione dei privilegi non ha restituito un handle di processo"],
    ["--yes authorizes downloading and installing Bitwarden from the Microsoft WinGet repository and accepting Bitwarden package licenses/agreements and WinGet source agreements. Another password manager is a valid choice.", "--yes autorizza il download e l'installazione di Bitwarden dal repository Microsoft WinGet e l'accettazione delle licenze e degli accordi del pacchetto Bitwarden e dell'origine WinGet. È possibile scegliere un altro gestore di password."],
    ["Bitwarden installation is supported only on Windows", "L'installazione di Bitwarden è supportata solo su Windows"],
    ["Defender preference is not a readable boolean", "L'impostazione di Defender non è un valore booleano leggibile"],
    ["Firewall enabled preference is not a concrete boolean", "L'impostazione di attivazione del firewall non è un valore booleano definito"],
    ["UAC value is not a DWORD", "Il valore UAC non è un DWORD"],
    ["UAC repair requires an explicitly disabled current setting", "La correzione di UAC richiede un'impostazione corrente esplicitamente disattivata"],
    ["Cannot read all effective firewall profiles", "Impossibile leggere tutti i profili effettivi del firewall"],
    ["No readable volume status", "Nessuno stato del volume leggibile"],
    ["Offline update query did not fully succeed", "La ricerca offline degli aggiornamenti non è riuscita completamente"],
    ["Secblitz requires Windows 10/11 x64; this platform cannot assess or change Windows", "Secblitz richiede Windows 10/11 x64; questa piattaforma non può valutare né modificare Windows"],
    ["Elevation is only supported on Windows", "L'elevazione dei privilegi è supportata solo su Windows"],
    ["Windows services are only supported on Windows", "I servizi Windows sono supportati solo su Windows"],
    ["Run JSON reports from an administrator terminal.", "Generare i rapporti JSON da un terminale amministratore."],
    ["Registered antivirus: ", "Antivirus registrato: "],
    ["; registered firewall: ", "; firewall registrato: "],
    ["Registration alone does not establish provider health. Additional or unrecognized registrations block the corresponding Defender/firewall changes, even when reported inactive.", "La sola registrazione non dimostra il corretto funzionamento del prodotto di sicurezza. Registrazioni aggiuntive o non riconosciute bloccano le relative modifiche a Defender o al firewall, anche se risultano inattive."],
    ["ActiveStore values are shown; NotConfigured does not establish the effective default action. Rules and outbound preferences are preserved.", "Sono mostrati i valori di ActiveStore; NotConfigured non indica quale sia l'azione predefinita effettiva. Le regole e le impostazioni del traffico in uscita vengono conservate."],
    ["Check Windows Security for effective protection and signature updates.", "Verificare la protezione effettiva e gli aggiornamenti delle firme in Sicurezza di Windows."],
    ["; hidden exclusions cannot be ruled out. Exclusions are preserved.", "; non è possibile escludere la presenza di esclusioni nascoste. Le esclusioni vengono conservate."],
    ["Standard Windows 10 support ended October 14, 2025. ESU enrollment and LTSC/IoT editions have different support terms; enrollment/support entitlement is not verified. Windows 11 support depends on release and edition; check Microsoft's lifecycle information.", "Il supporto standard di Windows 10 è terminato il 14 ottobre 2025. L'adesione a ESU e le edizioni LTSC/IoT prevedono condizioni diverse; l'adesione e il diritto al supporto non vengono verificati. Il supporto di Windows 11 dipende dalla versione e dall'edizione; consultare le informazioni Microsoft sul ciclo di vita."],
    ["Recovery-key backup is not verified.", "Il backup della chiave di ripristino non viene verificato."],
    ["Unsupported firmware or inaccessible status is reported as unknown.", "Un firmware non supportato o uno stato inaccessibile viene segnalato come sconosciuto."],
    ["Locally cached pending updates=", "Aggiornamenti in sospeso nella cache locale="],
    ["This offline result does not establish current patch compliance; open Windows Update and check for updates.", "Questo risultato offline non dimostra che le patch siano aggiornate; aprire Windows Update e cercare gli aggiornamenti."],
    ["Deny incoming Remote Desktop connections=", "Rifiuto delle connessioni Desktop remoto in entrata="],
    ["Review need, network exposure and Network Level Authentication; no changes made.", "Valutare la necessità, l'esposizione in rete e l'autenticazione a livello di rete; nessuna modifica effettuata."],
    ["SMB1 optional feature state=", "Stato della funzionalità facoltativa SMB1="],
    ["Review dependencies before removing legacy protocol support.", "Verificare le dipendenze prima di rimuovere il supporto dei protocolli obsoleti."],
    ["Enabled local accounts whose PasswordRequired flag is false: ", "Account locali attivi con indicatore PasswordRequired impostato su false: "],
    ["This flag does not reveal password presence, strength, reuse or Windows Hello security. Review account access and use strong unique passwords/MFA where supported.", "Questo indicatore non rivela la presenza, la robustezza o il riutilizzo delle password, né la sicurezza di Windows Hello. Verificare l'accesso agli account e usare password robuste e uniche e l'autenticazione a più fattori, se supportata."],
    ["Review Core isolation in Windows Security and driver compatibility before enabling.", "Prima dell'attivazione, verificare Isolamento core in Sicurezza di Windows e la compatibilità dei driver."],
    ["Mode=", "Modalità="],
    ["service=", "servizio="],
    ["antivirus=", "antivirus="],
    ["realtime=", "tempo reale="],
    ["behavior=", "comportamento="],
    ["archive preference disabled=", "scansione degli archivi disattivata nelle impostazioni="],
    ["tamper protected=", "protezione antimanomissione="],
    ["signatures=", "firme="],
    ["updated=", "aggiornate="],
    ["age days=", "giorni dall'aggiornamento="],
    ["Exclusion counts: paths=", "Numero di esclusioni: percorsi="],
    ["processes=", "processi="],
    ["extensions=", "estensioni="],
    ["enabled=", "attivo="],
    ["inbound=", "in entrata="],
    ["outbound=", "in uscita="],
    ["protection=", "protezione="],
    ["state=", "stato="],
    ["Secure Boot enabled=", "Avvio protetto attivo="],
    ["HVCI running=", "HVCI in esecuzione="],
    ["configured=", "configurato="],
    ["OS=", "SO="],
    ["version=", "versione="],
    ["build=", "build="],
    ["Unsupported Windows client capability", "Funzionalità del client Windows non supportata"],
    ["Domain membership is not readable", "Impossibile leggere l'appartenenza al dominio"],
    ["Domain-managed machine: assessment only", "Computer gestito da un dominio: solo valutazione"],
    ["Enrollment or cloud-management evidence: assessment only", "Rilevata registrazione o gestione tramite cloud: solo valutazione"],
    ["Configured management/security policy: assessment only", "Criterio di gestione o sicurezza configurato: solo valutazione"],
    ["Applied computer Group Policy: assessment only", "Criteri di gruppo applicati al computer: solo valutazione"],
    ["Local computer policy artifacts: assessment only", "Rilevati elementi dei criteri locali del computer: solo valutazione"],
    ["Additional or unrecognized security provider: assessment only", "Prodotto di sicurezza aggiuntivo o non riconosciuto: solo valutazione"],
    ["Defender provider registration cannot be established", "Impossibile confermare la registrazione di Defender come prodotto di sicurezza"],
    ["Defender tamper-protection state is unknown", "Lo stato della protezione antimanomissione di Defender non è noto"],
    ["Defender unavailable, passive, or tamper protected: assessment only", "Defender non disponibile, in modalità passiva o con protezione antimanomissione attiva: solo valutazione"],
    ["Firewall services unavailable", "Servizi del firewall non disponibili"],
    ["Firewall profile has resultant Group Policy: assessment only", "Il profilo del firewall presenta Criteri di gruppo risultanti: solo valutazione"],
    ["Firewall profile capability cannot be established", "Impossibile determinare le funzionalità disponibili per il profilo del firewall"],
    ["Conservative, reversible Windows hardening", "Rafforzamento prudente e reversibile della sicurezza di Windows"],
    ["Audit security preferences", "Verifica le impostazioni di sicurezza"],
    ["Apply conservative protection", "Applica misure di protezione prudenti"],
    ["Restore the latest recorded transaction", "Annulla l'ultima transazione registrata"],
    ["Show transaction history", "Mostra la cronologia delle transazioni"],
    ["Generate a 24-character password on this terminal only", "Genera una password di 24 caratteri solo in questo terminale"],
    ["Manage the optional service", "Gestisci il servizio facoltativo"],
    ["Optional software tools", "Strumenti software facoltativi"],
    ["Install the service", "Installa il servizio"],
    ["Uninstall the service", "Disinstalla il servizio"],
    ["Query service status", "Consulta lo stato del servizio"],
    ["Run the service dispatcher", "Esegui il dispatcher del servizio"],
    ["Install Bitwarden with explicit consent", "Installa Bitwarden con consenso esplicito"],
    ["Consent to downloading and installing Bitwarden", "Autorizza il download e l'installazione di Bitwarden"],
    ["Language (default: Windows display language)", "Lingua (predefinita: lingua di visualizzazione di Windows)"],
    ["Disable terminal animation", "Disattiva l'animazione del terminale"],
    ["Output raw JSON reports only (audit/apply/revert/history)", "Mostra solo rapporti JSON non tradotti (audit/apply/revert/history)"],
    ["Show help", "Mostra la guida"],
    ["Show version", "Mostra la versione"],
    ["Usage", "Uso"],
    ["Commands", "Comandi"],
    ["Options", "Opzioni"],
    ["No command: request administrator access and apply conservative protection automatically.", "Senza comando: richiede l'accesso come amministratore e applica automaticamente misure di protezione prudenti."],
    ["Working", "Operazione in corso"],
    ["Complete", "Completato"],
    ["Review needed", "Verifica necessaria"],
    ["Results", "Risultati"],
    ["Findings", "Esiti delle verifiche"],
    ["Details", "Dettagli"],
    ["Transaction", "Transazione"],
    ["History", "Cronologia"],
    ["No transactions recorded", "Nessuna transazione registrata"],
    ["No preference changes", "Nessuna modifica alle impostazioni"],
    ["Requesting administrator access", "Richiesta di accesso come amministratore"],
    ["Press Enter to close", "Premere Invio per chiudere"],
    ["Operation failed", "Operazione non riuscita"],
    ["Invalid command", "Comando non valido"],
    ["Use --help for usage.", "Usare --help per consultare la guida."],
    ["JSON is available only for audit, apply, revert and history.", "JSON è disponibile solo per audit, apply, revert e history."],
    ["Bitwarden installation requires --yes.", "L'installazione di Bitwarden richiede --yes."],
    ["Password output requires an interactive terminal.", "La visualizzazione della password richiede un terminale interattivo."],
    ["New password - save it in your password manager", "Nuova password - salvala nel tuo gestore di password"],
    ["Not saved or copied to the clipboard. Existing passwords were not inspected.", "Non salvata né copiata negli appunti. Le password esistenti non sono state esaminate."],
    ["compliant", "conforme"],
    ["attention", "richiede attenzione"],
    ["skipped", "omesso"],
    ["error", "errore"],
    ["unknown", "sconosciuto"],
    ["info", "informazioni"],
    ["ok", "corretto"],
    ["pending", "in sospeso"],
    ["unchanged", "invariato"],
    ["conflict", "conflitto"],
    ["applied", "applicato"],
    ["restored", "ripristinato"],
    ["reverted", "annullato"],
    ["reverting", "annullamento in corso"],
    ["Defender real-time protection", "Protezione in tempo reale di Defender"],
    ["Defender behavior monitoring", "Monitoraggio del comportamento di Defender"],
    ["Defender downloaded-file scanning", "Scansione dei file scaricati di Defender"],
    ["Defender archive scanning", "Scansione degli archivi di Defender"],
    ["Enable firewall", "Attiva il firewall"],
    ["Block unsolicited inbound traffic", "Blocca il traffico in entrata non richiesto"],
    ["domain", "dominio"],
    ["private", "privato"],
    ["public", "pubblico"],
    ["Enable UAC", "Attiva UAC"],
    ["Require administrator consent", "Richiedi il consenso dell'amministratore"],
    ["Security providers", "Prodotti di sicurezza"],
    ["Windows Firewall", "Firewall di Windows"],
    ["Windows lifecycle", "Ciclo di vita di Windows"],
    ["Device encryption", "Crittografia del dispositivo"],
    ["Secure Boot", "Avvio protetto"],
    ["Windows updates", "Aggiornamenti di Windows"],
    ["Remote Desktop", "Desktop remoto"],
    ["Local accounts", "Account locali"],
    ["Memory integrity", "Integrità della memoria"],
    ["Management and mutation eligibility", "Gestione e requisiti per le modifiche"],
    ["Journal recovery", "Recupero del registro delle modifiche"],
    ["Eligible unmanaged local preference", "Impostazione locale non gestita idonea alla modifica"],
    ["Preserving absent or nonzero UAC preference", "L'impostazione UAC assente o diversa da zero viene conservata"],
    ["Revert the active transaction before applying again", "Annullare la transazione attiva prima di applicare altre modifiche"],
    ["Target preference already present; original before image retained", "Impostazione prevista già presente; copia dello stato originale conservata"],
    ["Preference drifted; original before image retained", "L'impostazione è cambiata; copia dello stato originale conservata"],
    ["Revert the active transaction before starting another apply", "Annullare la transazione attiva prima di avviare una nuova applicazione delle modifiche"],
    ["Target preference already present", "Impostazione prevista già presente"],
    ["Preference applied", "Impostazione applicata"],
    ["; restart required", "; riavvio necessario"],
    ["Original preference already present", "Impostazione originale già presente"],
    ["Original preference restored", "Impostazione originale ripristinata"],
    ["Preference differs from both target and before image; no write performed", "L'impostazione differisce sia dal valore previsto sia dallo stato originale; nessuna scrittura eseguita"],
    ["Preference changed immediately before restore; no write performed", "L'impostazione è cambiata immediatamente prima del ripristino; nessuna scrittura eseguita"],
    [" remains unreverted; use revert to restore its recorded preferences.", " non è ancora stata annullata; usare revert per ripristinare le impostazioni registrate."],
    ["Assessment unavailable: ", "Valutazione non disponibile: "],
    ["No management evidence found by conservative local probes. Each mutation repeats management and control-specific capability checks.", "Le verifiche locali prudenti non hanno rilevato indizi di gestione. Prima di ogni modifica vengono ripetute le verifiche sulla gestione e sulle funzionalità specifiche del controllo."],
    ["Review reputation-based protection and SmartScreen in Windows Security and your browser. Per-user, browser and policy settings differ; effective protection is not inferred from a single registry value.", "Verificare la protezione basata sulla reputazione e SmartScreen in Sicurezza di Windows e nel browser. Le impostazioni utente, del browser e dei criteri sono distinte; la protezione effettiva non viene dedotta da un singolo valore del Registro di sistema."],
    // Impact phrases
    ["A full drive stopping fixes and updates from completing", "Un disco pieno che impedisce il completamento di correzioni e aggiornamenti"],
    ["Malware running as soon as it lands on your PC", "Malware che si avvia non appena arriva sul tuo PC"],
    ["Apps that behave like malware even when not yet known", "App che si comportano come malware anche se non ancora note"],
    ["Harmful files downloaded from the web or email attachments", "File dannosi scaricati dal web o come allegati e-mail"],
    ["Malware hidden inside zip and other compressed files", "Malware nascosto in file zip e altri file compressi"],
    ["Other devices on your work network reaching your PC", "Altri dispositivi sulla rete aziendale che accedono al tuo PC"],
    ["Other devices on your home network reaching your PC", "Altri dispositivi sulla rete domestica che accedono al tuo PC"],
    ["Other devices at public places like cafes or airports reaching your PC", "Altri dispositivi in luoghi pubblici come bar o aeroporti che accedono al tuo PC"],
    ["Uninvited incoming connections on your work network", "Connessioni in entrata non richieste sulla rete aziendale"],
    ["Uninvited incoming connections on your home network", "Connessioni in entrata non richieste sulla rete domestica"],
    ["Uninvited incoming connections on public networks like cafes or airports", "Connessioni in entrata non richieste su reti pubbliche come bar o aeroporti"],
    ["Apps silently making system-wide changes without asking you", "App che modificano il sistema senza chiedere il tuo permesso"],
    ["Apps making administrator changes without asking for approval", "App che apportano modifiche da amministratore senza chiedere approvazione"],
    ["Any app installer quietly getting full control of your PC", "Qualsiasi programma di installazione che ottiene silenziosamente il controllo totale del tuo PC"],
    ["Strangers on the network listing your account names to guess passwords", "Estranei in rete che elencano i tuoi nomi account per indovinare le password"],
    ["Someone signing in over the network to an account with no password", "Qualcuno che accede tramite rete a un account senza password"],
    ["Attackers stealing your Windows password from memory", "Aggressori che rubano la tua password di Windows dalla memoria"],
    ["Tampered or fake Windows updates reaching your PC", "Aggiornamenti di Windows falsificati o manomessi che raggiungono il tuo PC"],
    ["Running Windows that no longer gets security fixes", "Usare una versione di Windows che non riceve più correzioni di sicurezza"],
    ["Strangers reading your files if your PC is lost or stolen", "Estranei che leggono i tuoi file se il PC viene perso o rubato"],
    ["Hidden malware loading before Windows starts", "Malware nascosto che si carica prima dell'avvio di Windows"],
    ["Known security holes staying open on your PC", "Vulnerabilità di sicurezza note che restano aperte sul tuo PC"],
    ["Attackers trying to sign in to your PC remotely", "Aggressori che tentano di accedere al tuo PC da remoto"],
    ["Old file-sharing flaws used by worms like WannaCry", "Vecchie vulnerabilità nella condivisione dei file sfruttate da worm come WannaCry"],
    ["Scam websites and unrecognized apps you open by mistake", "Siti fraudolenti e app non riconosciute che apri per sbaglio"],
    ["Weak or shared sign-ins that are easier to guess or steal", "Credenziali deboli o condivise più facili da indovinare o sottrarre"],
    ["Malicious drivers taking over the core of Windows", "Driver dannosi che prendono il controllo del nucleo di Windows"],
    ["Anyone who turns on your PC getting straight into your account", "Chiunque accenda il tuo PC accedendo direttamente al tuo account"],
    // Impact prefix keys
    ["Risk:", "Rischio:"],
    ["Protects you from:", "Ti protegge da:"],
    ["Why it matters:", "Perché è importante:"],
    // Column header
    ["Why it matters / Next step", "Perché è importante / Passaggio successivo"],
    // Payoff section headings
    ["You're now protected from:", "Ora sei protetto da:"],
    ["After you restart, you'll be protected from:", "Dopo il riavvio, sarai protetto da:"],
    // Recap note for restart-required fixes
    ["Needs a restart to finish", "Richiede un riavvio per completarsi"],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn all_keys() -> impl Iterator<Item = &'static str> {
        TEXT.iter()
            .map(|r| r[0])
            .chain(MAINTENANCE_TEXT.iter().map(|r| r[0]))
    }

    #[test]
    fn maintenance_catalog_covers_all_six_locales_without_ambiguous_keys() {
        let mut keys: std::collections::HashSet<_> = TEXT.iter().map(|r| r[0]).collect();
        for row in MAINTENANCE_TEXT {
            assert!(keys.insert(row[0]), "duplicate maintenance key: {}", row[0]);
            for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                assert!(!row[lang as usize].is_empty());
                assert!(!row[lang as usize].contains('\u{2014}'));
                assert_eq!(lang.t(row[0]), row[lang as usize]);
                assert_eq!(lang.detail(row[0]), row[lang as usize]);
            }
        }
    }

    #[test]
    fn permission_evidence_and_identifiers_survive_all_languages() {
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for control in secblitz::permissions::controls() {
                assert_ne!(lang.control(&control.id), control.id);
                assert_eq!(
                    lang.control(&control.id),
                    lang.t(crate::ui::advice::control_label(&control.id))
                );
            }
            for name in [
                "BITS",
                "wuauserv",
                "WinDefend",
                "Schedule",
                "SecblitzMonitor",
            ] {
                assert_eq!(
                    lang.detail(&format!("Service permissions: {name}")),
                    format!("{}{name}", lang.t("Service permissions: "))
                );
            }
            let evidence = "S-1-5-32-545: ALLOW mask 0x000d0002, risky bits 0x000d0002";
            let detail = lang.detail(&format!("Candidate dangerous broad-principal grants: {evidence}. This is an ACE scan, not effective access or proof of exploitability. Consult the fixed service repair control for gated eligibility."));
            for raw in ["S-1-5-32-545", "0x000d0002"] {
                assert!(detail.contains(raw));
            }
            assert!(detail.contains(&lang.t("ALLOW mask")));
            assert!(detail.contains(&lang.t("risky bits")));
            let native = "Access is denied (os error 5)";
            let rendered = lang.detail(&format!("Service permissions preserved: {native}"));
            assert!(rendered.starts_with(&lang.t("Service permissions preserved: ")));
            assert!(rendered.contains("Access is denied"));
            assert!(rendered.contains('5'));
            let ids = "permissions.service.bits permissions.service.wuauserv review_flag C:\\review\\errorlog S-1-5-11 0x80070005";
            assert_eq!(lang.detail(ids), ids);
            if lang != Lang::En {
                assert_ne!(lang.t("review"), "review");
                assert!(!detail.contains("Consult the fixed service repair control"));
            }
        }
    }

    #[test]
    fn windows_italian_regional_variants_are_detected() {
        for id in [0x0010, 0x0410, 0x0810] {
            assert_eq!(Lang::from_windows_language(id), Lang::It);
        }
        assert_eq!(Lang::from_windows_language(0x0411), Lang::En);
    }

    #[test]
    fn privilege_controls_and_autologon_evidence_are_localized() {
        for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for id in [
                "installer.always_install_elevated",
                "lsa.restrict_anonymous_sam",
                "lsa.limit_blank_password_use",
                "wdigest.use_logon_credential",
            ] {
                assert_ne!(lang.control(id), id);
                assert_ne!(lang.control(id), Lang::En.control(id));
            }
            let detail = lang.detail("AutoAdminLogon enabled=True; Winlogon DefaultPassword value present=False. Presence only: no password data is read. LSA-secret autologon storage is not inspected. Review physical access and credential exposure; automatic logon is preserved to avoid disrupting kiosk or sign-in workflows.");
            assert!(detail.contains("True"));
            assert!(detail.contains("False"));
            assert!(!detail.contains("no password data is read"));
        }
    }
    // A deliberately small lexer for source-catalog coverage, not a Rust parser.
    // Production files use ordinary string literals for diagnostic messages.
    fn literals(source: &str) -> Vec<String> {
        let source = source
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .split("#[cfg(all(test,")
            .next()
            .unwrap();
        let mut chars = source.chars().peekable();
        let mut out = Vec::new();
        let mut previous = '\0';
        while let Some(c) = chars.next() {
            if c == '/' && chars.peek() == Some(&'/') {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                previous = '\n';
                continue;
            }
            if c == '"' && previous != '\'' {
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        break;
                    }
                    text.push(c);
                    if c == '\\' {
                        if let Some(c) = chars.next() {
                            text.push(c);
                        }
                    }
                }
                out.push(text);
            }
            previous = c;
        }
        out
    }

    #[test]
    fn fixed_rust_diagnostic_prose_has_catalog_coverage() {
        let mut missing = Vec::new();
        for (name, source) in [
            ("main", include_str!("main.rs")),
            ("maintenance_cli", include_str!("maintenance_cli.rs")),
            ("guided", include_str!("guided.rs")),
            ("menu", include_str!("menu.rs")),
            ("advice", include_str!("advice.rs")),
            ("actions", include_str!("actions.rs")),
            ("actions/windows", include_str!("actions/windows.rs")),
            ("ui", include_str!("ui.rs")),
            ("tools", include_str!("tools.rs")),
            ("engine", include_str!("engine.rs")),
            ("model", include_str!("model.rs")),
            ("readiness", include_str!("readiness.rs")),
            ("readiness/windows", include_str!("readiness/windows.rs")),
            ("platform", include_str!("platform.rs")),
            ("windows", include_str!("platform/windows.rs")),
            ("journal", include_str!("platform/journal.rs")),
            ("service", include_str!("service.rs")),
            ("service/windows", include_str!("service/windows.rs")),
            ("permissions", include_str!("permissions.rs")),
            ("permissions/state", include_str!("permissions/state.rs")),
            (
                "permissions/descriptor",
                include_str!("permissions/descriptor.rs"),
            ),
            (
                "permissions/windows",
                include_str!("permissions/windows.rs"),
            ),
        ] {
            for text in literals(source) {
                // Some presentation keys intentionally contain substitution
                // markers. Check the complete key before stripping Rust fields.
                if all_keys().any(|key| key == text) {
                    continue;
                }
                if !text.contains(' ')
                    || text.contains('\\')
                    || text.starts_with("[Console]")
                    || text.starts_with("$global:ProgressPreference = ")
                    || text == "Secblitz · v{}"
                    // Exact service-host argument allowlist, not application prose.
                    || matches!(text.as_str(), " -k netsvcs" | " -k netsvcs -p" | "Windows Update")
                {
                    continue;
                }
                let mut rest = text.as_str();
                let mut leftover = String::new();
                while !rest.is_empty() {
                    if rest.starts_with('{') {
                        if let Some(end) = rest.find('}') {
                            rest = &rest[end + 1..];
                            continue;
                        }
                    }
                    if let Some(key) = all_keys()
                        .filter(|key| rest.starts_with(key))
                        .max_by_key(|key| key.len())
                    {
                        rest = &rest[key.len()..];
                    } else {
                        let c = rest.chars().next().unwrap();
                        leftover.push(c);
                        rest = &rest[c.len_utf8()..];
                    }
                }
                let words: Vec<_> = leftover
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .filter(|s| s.chars().any(|c| c.is_ascii_alphabetic()))
                    .filter(|s| {
                        ![
                            "WinGet",
                            "Win32",
                            "HRESULT",
                            "GetPackagesByPackageFamily",
                            "SecblitzMonitor",
                            "BITS",
                            "wuauserv",
                            "WinDefend",
                            "Schedule",
                            "LocalService",
                            "SCM",
                            "SID",
                            "0x",
                            "s",     // SI time suffix next to a formatted duration.
                            "bytes", // Native byte counts in the catalog listing.
                        ]
                        .contains(s)
                    })
                    .collect();
                if !words.is_empty() {
                    missing.push(format!("{name}: {text} => {words:?}"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "Untranslated application prose:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn scoped_policy_messages_render_in_every_language() {
        let messages = [
            "Relevant policy is configured or its authority is unknown: assessment only",
            "Group Policy authority is unknown: assessment only",
            "Relevant resultant Group Policy: assessment only",
            "Firewall preference/effective readback did not match; mutation outcome requires review",
            "No device-management registration or UAC policy authority found by the available probes. Each control repeats scoped policy and capability checks before mutation.",
        ];
        for message in messages {
            assert!(include_str!("platform/backend.ps1").contains(message));
            for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                let translated = lang.t(message);
                if lang != Lang::En {
                    assert_ne!(translated, message, "{}: {message}", lang.code());
                }
                // The report/error rendering path also sees prefixes and native evidence.
                let input = format!(
                    "Assessment unavailable: {message} [firewall.public.inbound; 0x80041003]"
                );
                let rendered = lang.detail(&input);
                assert!(rendered.contains(&translated));
                assert!(rendered.starts_with(&lang.t("Assessment unavailable: ")));
                assert!(rendered.ends_with("[firewall.public.inbound; 0x80041003]"));
            }
        }
    }

    #[test]
    fn backend_fixed_errors_titles_and_advice_are_translated() {
        let source = include_str!("platform/backend.ps1");
        for marker in [
            "throw '",
            "ThrowGate '",
            "Finding '",
            "title='",
            "detail='",
            "reason='",
        ] {
            for part in source.split(marker).skip(1) {
                let key = part.split('\'').next().unwrap();
                // Product/protocol names and diagnostic values are language-neutral.
                if ["Defender", "SMB1", "SmartScreen"].contains(&key) {
                    continue;
                }
                assert!(
                    TEXT.iter().any(|row| row[0] == key),
                    "Missing backend translation: {key}"
                );
            }
        }
        for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let detail = lang.detail("Apply firewall.public.inbound has unknown outcome; pending transaction 00042-abc retained");
            assert!(detail.contains("firewall.public.inbound"));
            assert!(detail.contains("00042-abc"));
            assert!(!detail.contains("has unknown outcome"));
            assert!(!detail.contains("retained"));
        }
    }

    #[test]
    fn defender_action_script_diagnostics_have_exact_translations() {
        for part in include_str!("actions/defender.ps1")
            .split("throw '")
            .skip(1)
        {
            let key = part.split('\'').next().unwrap();
            assert!(
                TEXT.iter().any(|row| row[0] == key),
                "Missing action diagnostic: {key}"
            );
            for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                assert_ne!(lang.t(key), key, "{}: {key}", lang.code());
            }
        }
    }

    #[test]
    fn guided_fixed_copy_uses_whole_keys_and_localized_keyboard_hints() {
        for key in literals(include_str!("guided.rs"))
            .into_iter()
            .chain(literals(include_str!("menu.rs")))
        {
            if (key.contains(' ') || matches!(key.as_str(), "Back" | "Exit"))
                && !key.contains('{')
                && !key.contains('\\')
                && key.chars().any(|c| c.is_ascii_alphabetic())
            {
                assert!(
                    all_keys().any(|entry| entry == key),
                    "Missing exact guided/menu key: {key}"
                );
            }
        }
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let choose = lang.t(crate::menu::SELECT_HINT);
            let select = lang.t(crate::menu::MULTI_HINT);
            let (enter, space, escape) = match lang {
                Lang::En => ("Enter", "Space", "Esc"),
                Lang::Es => ("Intro", "Espacio", "Esc"),
                Lang::Fr => ("Entrée", "Espace", "Échap"),
                Lang::De => ("Eingabe", "Leertaste", "Esc"),
                Lang::Pt => ("Enter", "Espaço", "Esc"),
                Lang::It => ("Invio", "Spazio", "Esc"),
            };
            for hint in [&choose, &select] {
                assert!(hint.contains("↑/↓"));
                assert!(hint.contains(enter));
                assert!(hint.contains(escape));
            }
            assert!(select.contains(space));
            assert_ne!(lang.t("Yes, continue"), lang.t("No, go back"));
            if lang != Lang::En {
                for key in [
                    crate::menu::SELECT_HINT,
                    crate::menu::MULTI_HINT,
                    "Yes, continue",
                    "No, go back",
                    "Choose an action",
                    "Choose what to fix",
                    "Select the fixes you want.",
                    "Invalid menu default",
                    "Invalid menu selection",
                    "Terminal is too short to display a menu",
                    "Operation failed",
                ] {
                    assert_ne!(lang.t(key), key, "{}: {key}", lang.code());
                }
            }
            for key in DETAIL_EXACT_ONLY {
                assert_eq!(lang.detail(key), lang.t(key));
            }
            assert_eq!(
                lang.detail("native all none complete opened returned running"),
                "native all none complete opened returned running"
            );
            let evidence = "permissions.service.bits ms-settings:windowsupdate C:\\all\\complete.exe 0x80070005";
            assert_eq!(lang.detail(evidence), evidence);
        }
    }

    #[test]
    fn keyboard_sources_do_not_prompt_for_typed_menu_numbers() {
        for (name, source) in [
            ("guided", include_str!("guided.rs")),
            ("menu", include_str!("menu.rs")),
        ] {
            for key in literals(source) {
                let numbered_label =
                    key.starts_with('[') && key.as_bytes().get(1).is_some_and(u8::is_ascii_digit);
                assert!(!numbered_label, "Numbered prompt in {name}: {key}");
                for obsolete in [
                    "Enter numbers separated by commas",
                    "displayed menu numbers",
                    "displayed numbers, all, or none",
                ] {
                    assert!(
                        !key.contains(obsolete),
                        "Typed-number prompt in {name}: {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn automatic_flow_and_readiness_copy_is_complete_and_preserves_placeholders() {
        let mut text_block = false;
        for key in include_str!("../docs/review-auto-flow.md").lines() {
            if key == "```text" {
                text_block = true;
                continue;
            }
            if key == "```" {
                text_block = false;
                continue;
            }
            if !text_block || key.is_empty() {
                continue;
            }
            assert!(
                TEXT.iter().any(|row| row[0] == key),
                "Missing automatic-flow key: {key}"
            );
            for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                assert_ne!(lang.t(key), key, "{}: {key}", lang.code());
            }
        }
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for (key, placeholder, value) in [
                ("{gb} GB free", "{gb}", "1099.5"),
                ("Battery: {percent}%", "{percent}", "20"),
            ] {
                let text = lang.t(key);
                assert_eq!(text.matches(placeholder).count(), 1);
                let rendered = text.replace(placeholder, value);
                assert!(rendered.contains(value));
                assert!(!rendered.contains(['{', '}']));
            }
            for key in ["EffectiveFirewallMismatch", "EffectiveFirewallUnavailable"] {
                assert!(TEXT.iter().any(|row| row[0] == key));
                let raw_id = format!("native_{key} C:\\{key}\\evidence.json");
                assert_eq!(lang.detail(&raw_id), raw_id);
            }
        }
    }

    #[test]
    fn report_handoff_copy_has_exact_catalog_entries() {
        let mut text_block = false;
        for line in include_str!("../docs/report-copy-keys.md").lines() {
            if line == "```text" {
                text_block = true;
                continue;
            }
            if line == "```" {
                text_block = false;
                continue;
            }
            if text_block && !line.is_empty() {
                assert!(
                    TEXT.iter().any(|row| row[0] == line),
                    "Missing report key: {line}"
                );
            }
        }
    }
    #[test]
    fn catalog_is_complete_and_unambiguous() {
        for (name, source) in [
            ("i18n", include_str!("i18n.rs")),
            (
                "guided-copy-keys",
                include_str!("../docs/guided-copy-keys.md"),
            ),
        ] {
            assert!(
                !source.contains('\u{2014}'),
                "U+2014 is not permitted in {name}"
            );
        }
        let mut keys = std::collections::HashSet::new();
        for row in TEXT {
            assert!(keys.insert(row[0]), "duplicate source: {}", row[0]);
            assert!(
                row.iter().all(|s| !s.is_empty()),
                "missing translation: {}",
                row[0]
            );
            assert!(
                row.iter().all(|s| !s.contains('\u{2014}')),
                "em dash in catalog: {}",
                row[0]
            );
        }
        let mut italian_keys = std::collections::HashSet::new();
        for [key, translated] in ITALIAN {
            assert!(italian_keys.insert(*key), "duplicate Italian source: {key}");
            assert!(!translated.is_empty(), "missing Italian translation: {key}");
            assert!(
                !key.contains('\u{2014}') && !translated.contains('\u{2014}'),
                "em dash in Italian catalog: {key}"
            );
            // These technical terms have the same spelling in Italian.
            if !["antivirus=", "build="].contains(key) {
                assert_ne!(key, translated, "Italian placeholder: {key}");
            }
        }
        assert_eq!(
            keys, italian_keys,
            "Italian catalog must cover every source key"
        );
        for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            for row in TEXT {
                assert_eq!(
                    lang.detail(row[0]),
                    lang.translation(row),
                    "rendered catalog key: {}",
                    row[0]
                );
            }
            assert_eq!(Lang::parse(lang.code()), Some(lang));
            assert!(!lang
                .control("firewall.private.inbound")
                .contains("firewall.private"));
        }
    }
    #[test]
    fn dynamic_evidence_survives_prose_translation() {
        for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
            let text = lang.detail("Transaction 00042-abc remains unreverted; use revert to restore its recorded preferences.");
            assert!(text.contains("00042-abc"));
            assert!(!text.contains("remains unreverted"));
            let text = lang.detail("Preference applied; restart required");
            assert!(!text.contains("Preference applied"));
            assert!(!text.contains("restart required"));
            assert_eq!(
                lang.detail("Notebook information public_key errorlog"),
                "Notebook information public_key errorlog"
            );
            assert_eq!(
                lang.control("firewall.private.future"),
                "firewall.private.future"
            );
        }
    }

    #[test]
    fn advice_impact_keys_are_translated_in_all_six_languages() {
        // All non-empty control_impact phrases must be translated.
        let control_ids = [
            "defender.realtime", "defender.behavior", "defender.ioav", "defender.archive",
            "firewall.domain.enabled", "firewall.private.enabled", "firewall.public.enabled",
            "firewall.domain.inbound", "firewall.private.inbound", "firewall.public.inbound",
            "uac.enabled", "uac.consent", "installer.always_install_elevated",
            "lsa.restrict_anonymous_sam", "lsa.limit_blank_password_use",
            "wdigest.use_logon_credential", "permissions.service.bits",
            "permissions.service.wuauserv",
        ];
        for id in control_ids {
            let impact = crate::ui::advice::control_impact(id);
            assert!(!impact.is_empty(), "control_impact({id}) is empty");
            assert!(
                TEXT.iter().any(|row| row[0] == impact),
                "impact phrase not in TEXT catalog: {impact}"
            );
            for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                let translated = lang.t(impact);
                assert!(!translated.is_empty(), "empty translation for {id} impact in {}", lang.code());
                if lang != Lang::En {
                    assert_ne!(translated, impact, "{}: impact phrase not translated for {id}", lang.code());
                }
            }
        }
        // All non-empty finding_impact phrases must be translated.
        let finding_titles = [
            "Windows lifecycle", "Device encryption", "Secure Boot", "Windows updates",
            "Remote Desktop", "SMB1", "SmartScreen", "Local accounts", "Memory integrity",
            "Automatic logon",
        ];
        for title in finding_titles {
            let impact = crate::ui::advice::finding_impact(title);
            assert!(!impact.is_empty(), "finding_impact({title}) is empty");
            assert!(
                TEXT.iter().any(|row| row[0] == impact),
                "finding impact phrase not in TEXT catalog: {impact}"
            );
            for lang in [Lang::En, Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                let translated = lang.t(impact);
                assert!(!translated.is_empty(), "empty translation for {title} impact in {}", lang.code());
                if lang != Lang::En {
                    assert_ne!(translated, impact, "{}: finding impact phrase not translated for {title}", lang.code());
                }
            }
        }
        // Impact prefix keys are translated in non-English languages.
        for prefix in ["Risk:", "Protects you from:", "Why it matters:"] {
            assert!(
                TEXT.iter().any(|row| row[0] == prefix),
                "prefix not in TEXT catalog: {prefix}"
            );
            for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                let translated = lang.t(prefix);
                assert_ne!(translated, prefix, "{}: prefix not translated: {prefix}", lang.code());
                assert!(!translated.is_empty());
            }
        }
        // Column header and payoff headings are translated.
        for key in [
            "Why it matters / Next step",
            "You're now protected from:",
            "After you restart, you'll be protected from:",
        ] {
            assert!(TEXT.iter().any(|row| row[0] == key), "key not in catalog: {key}");
            for lang in [Lang::Es, Lang::Fr, Lang::De, Lang::Pt, Lang::It] {
                let translated = lang.t(key);
                assert_ne!(translated, key, "{}: key not translated: {key}", lang.code());
                assert!(!translated.is_empty());
            }
        }
    }
}
