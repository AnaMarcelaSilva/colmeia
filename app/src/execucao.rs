//! O "Play" dos projetos: as configurações de execução (um comando do shell
//! numa subpasta, com variáveis de ambiente), como as do IntelliJ. O botão
//! fica no quadro de um projeto e no topo da tarefa (lá, roda na pasta da
//! tarefa); a saída aparece num painel embaixo, com Parar e Rodar de novo.
//! Quem guarda e roda é o núcleo: fechar a tela não para o que está rodando.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use eframe::egui::{self, RichText};

use crate::api::{self, CamposComando, Comando, Sugestoes, Variavel};
use crate::dados::Evento;
use crate::tema::{self, TipoAviso, cores};
use crate::terminal::TerminalAgente;

/// Um aviso para a tela principal mostrar.
pub type Aviso = (TipoAviso, String);

/// Rótulo do botão: o nome da configuração, cortado.
const NOME_NO_BOTAO: usize = 28;

#[derive(Default)]
pub struct Play {
    /// As configurações de cada projeto, buscadas quando aparecem.
    comandos: HashMap<i64, Vec<Comando>>,
    /// A configuração escolhida em cada projeto (a do clique no botão).
    escolhido: HashMap<i64, i64>,
    /// A configuração cujo terminal está no painel de baixo, e o projeto dela.
    pub painel: Option<i64>,
    projeto_do_painel: i64,
    terminais: HashMap<i64, TerminalAgente>,
    /// O tamanho do painel, para a próxima execução já nascer nele.
    tamanho: Option<(u16, u16)>,
    dialogo: Option<Configurar>,
}

/// A janela "Configurações de execução" de um projeto.
struct Configurar {
    projeto: i64,
    nome_projeto: String,
    /// Configuração em edição: o id (0 para uma nova) e os campos como texto.
    editando: Option<(i64, Edicao)>,
    sugestoes: Result<Sugestoes, String>,
    erro: Option<String>,
}

#[derive(Default)]
struct Edicao {
    nome: String,
    comando: String,
    pasta: String,
    /// Uma variável por linha, NOME=valor.
    ambiente: String,
}

impl Edicao {
    fn de(c: &CamposComando) -> Self {
        let ambiente = c.ambiente.iter().map(|v| format!("{}={}", v.nome, v.valor)).collect::<Vec<_>>().join("\n");
        Edicao { nome: c.nome.clone(), comando: c.comando.clone(), pasta: c.pasta.clone(), ambiente }
    }

    fn campos(&self) -> Result<CamposComando, String> {
        let mut ambiente = Vec::new();
        for linha in self.ambiente.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let Some((nome, valor)) = linha.split_once('=') else {
                return Err(format!("Variável sem \"=\": {linha}"));
            };
            ambiente.push(Variavel { nome: nome.trim().to_string(), valor: valor.to_string() });
        }
        Ok(CamposComando { nome: self.nome.trim().to_string(), comando: self.comando.trim().to_string(), pasta: self.pasta.trim().to_string(), ambiente })
    }
}

/// O estado de uma configuração, curto, para o menu e o painel.
fn estado(c: &Comando) -> Option<String> {
    if c.rodando {
        return Some("rodando".into());
    }
    c.fim.as_ref().map(|f| match (f.parada, f.codigo) {
        (true, _) => format!("parada às {}", f.hora),
        (false, 0) => format!("terminou às {}", f.hora),
        (false, n) => format!("código {n} às {}", f.hora),
    })
}

fn cortar(nome: &str) -> String {
    if nome.chars().count() <= NOME_NO_BOTAO { nome.to_string() } else { format!("{}…", nome.chars().take(NOME_NO_BOTAO - 1).collect::<String>()) }
}

impl Play {
    /// As configurações do projeto (busca na primeira vez).
    fn lista(&mut self, projeto: i64) -> &[Comando] {
        self.comandos.entry(projeto).or_insert_with(|| api::comandos(projeto).unwrap_or_default())
    }

    fn comando(&self, id: i64) -> Option<&Comando> {
        self.comandos.values().flatten().find(|c| c.id == id)
    }

    fn comando_mut(&mut self, id: i64) -> Option<&mut Comando> {
        self.comandos.values_mut().flatten().find(|c| c.id == id)
    }

    /// A escolhida do projeto: a última rodada ou escolhida, senão a primeira.
    fn escolhida(&mut self, projeto: i64) -> Option<Comando> {
        let escolhido = self.escolhido.get(&projeto).copied();
        let lista = self.lista(projeto);
        escolhido.and_then(|id| lista.iter().find(|c| c.id == id)).or_else(|| lista.first()).cloned()
    }

    /// Esquece o que sabe (o núcleo pode ter reiniciado): busca de novo.
    pub fn recarregar(&mut self) {
        self.comandos.clear();
    }

    /// Começou, terminou ou foi removida: busca a lista do projeto de novo.
    /// Uma que saiu com erro (sem ser parada) vira aviso.
    pub fn evento(&mut self, e: &Evento) -> Option<Aviso> {
        match e {
            // Atualiza na lista que já tem; sem ela, busca de novo.
            Evento::ComandoIniciou { comando_id, projeto_id, .. } => match self.comando_mut(*comando_id) {
                Some(c) => {
                    c.rodando = true;
                    c.fim = None;
                }
                None => {
                    self.comandos.remove(projeto_id);
                }
            },
            Evento::ComandoTerminou { comando_id, projeto_id, codigo, parada, hora, .. } => {
                let nome = match self.comando_mut(*comando_id) {
                    Some(c) => {
                        c.rodando = false;
                        c.fim = Some(api::FimExecucao { codigo: *codigo, parada: *parada, hora: hora.clone() });
                        c.campos.nome.clone()
                    }
                    None => {
                        self.comandos.remove(projeto_id);
                        "A execução".into()
                    }
                };
                if *codigo != 0 && !*parada {
                    return Some((TipoAviso::Alerta, format!("{nome} saiu com código {codigo}")));
                }
            }
            Evento::ComandoRemovido { comando_id, projeto_id, .. } => {
                self.comandos.remove(projeto_id);
                self.terminais.remove(comando_id);
                if self.painel == Some(*comando_id) {
                    self.painel = None;
                }
            }
            _ => {}
        }
        None
    }

    /// Roda (ou roda de novo) e mostra a saída no painel.
    pub fn rodar(&mut self, ctx: &egui::Context, bytes: &Arc<AtomicU64>, id: i64, tarefa: i64) -> Option<Aviso> {
        match api::rodar_comando(id, tarefa, self.tamanho) {
            Ok(c) => {
                self.escolhido.insert(c.projeto_id, id);
                self.projeto_do_painel = c.projeto_id;
                // Já rodando na lista; o evento traz o resto.
                if let Some(lista) = self.comandos.get_mut(&c.projeto_id)
                    && let Some(antiga) = lista.iter_mut().find(|a| a.id == id)
                {
                    *antiga = c;
                }
                ctx.request_repaint();
                // A execução nova é outro terminal no núcleo: conecta de novo.
                self.terminais.insert(id, TerminalAgente::conectar_independente(format!("/v1/comandos/{id}/terminal"), ctx.clone(), bytes.clone()));
                self.painel = Some(id);
                if let Some(t) = self.terminais.get(&id) {
                    t.focar();
                }
                None
            }
            Err(e) => Some((TipoAviso::Erro, format!("Não consegui rodar: {e}"))),
        }
    }

    pub fn parar(&mut self, id: i64) -> Option<Aviso> {
        api::parar_comando(id).err().map(|e| (TipoAviso::Erro, format!("Não consegui parar: {e}")))
    }

    /// Mostra o painel com a última execução da configuração.
    fn mostrar_no_painel(&mut self, ctx: &egui::Context, bytes: &Arc<AtomicU64>, id: i64) {
        if let Some(c) = self.comando(id) {
            self.projeto_do_painel = c.projeto_id;
        }
        self.terminais.entry(id).or_insert_with(|| TerminalAgente::conectar_independente(format!("/v1/comandos/{id}/terminal"), ctx.clone(), bytes.clone()));
        self.painel = Some(id);
    }

    /// Para a escolhida do projeto (Ctrl+F2) ou, sem projeto, a do painel.
    pub fn parar_atual(&mut self, projeto: Option<i64>) -> Option<Aviso> {
        let id = match projeto {
            Some(p) => self.escolhida(p).filter(|c| c.rodando).map(|c| c.id),
            None => None,
        }
        .or(self.painel.filter(|id| self.comando(*id).is_some_and(|c| c.rodando)))?;
        self.parar(id)
    }

    /// Roda a escolhida do projeto (Shift+F10); sem nenhuma, abre as configurações.
    pub fn rodar_escolhida(&mut self, ctx: &egui::Context, bytes: &Arc<AtomicU64>, projeto: i64, nome: &str, tarefa: i64) -> Option<Aviso> {
        match self.escolhida(projeto) {
            Some(c) => self.rodar(ctx, bytes, c.id, tarefa),
            None => {
                self.abrir_configuracoes(projeto, nome);
                None
            }
        }
    }

    pub fn abrir_configuracoes(&mut self, projeto: i64, nome: &str) {
        let sugestoes = api::sugestoes_de_comandos(projeto);
        let vazio = self.lista(projeto).is_empty();
        // Sem nenhuma e sem nada achado: já abre uma nova para escrever.
        let editando = (vazio && sugestoes.as_ref().is_ok_and(|s| s.sugestoes.is_empty())).then(|| (0, Edicao::default()));
        self.dialogo = Some(Configurar { projeto, nome_projeto: nome.to_string(), editando, sugestoes, erro: None });
    }

    /// Largura do botão, para quem reserva espaço na fileira.
    pub fn largura_botao(&mut self, pintor: &egui::Painter, projeto: i64) -> f32 {
        tema::largura_botao_dividido(pintor, &self.rotulo(projeto))
    }

    fn rotulo(&mut self, projeto: i64) -> String {
        match self.escolhida(projeto) {
            Some(c) if c.rodando => format!("Rodando · {}", cortar(&c.campos.nome)),
            Some(c) => format!("Rodar · {}", cortar(&c.campos.nome)),
            None => "Rodar…".to_string(),
        }
    }

    /// O botão dividido: à esquerda roda a escolhida (ou, rodando, mostra a
    /// saída); o menu escolhe outra, para, roda de novo e configura.
    #[allow(clippy::too_many_arguments)]
    pub fn botao(&mut self, ui: &mut egui::Ui, bytes: &Arc<AtomicU64>, projeto: i64, nome: &str, tarefa: i64, pode: bool) -> Option<Aviso> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let rotulo = self.rotulo(projeto);
        let escolhida = self.escolhida(projeto);
        let ponto = escolhida.as_ref().filter(|c| c.rodando).map(|_| p.ok);
        let (principal, menu) = tema::botao_dividido(ui, &rotulo, ponto);
        let onde = if tarefa != 0 { "na pasta da tarefa" } else { "na pasta do projeto" };
        let dica = match &escolhida {
            Some(c) if c.rodando => format!("Ver a saída de {}", c.campos.nome),
            Some(c) => format!("Rodar {} {onde} (Shift+F10)\n{}", c.campos.nome, c.campos.comando),
            None => "Configurar o que rodar neste projeto (como no IntelliJ)".to_string(),
        };
        let mut aviso = None;
        if principal.on_hover_text(dica).clicked() && pode {
            match &escolhida {
                Some(c) if c.rodando => self.mostrar_no_painel(&ctx, bytes, c.id),
                Some(c) => aviso = self.rodar(&ctx, bytes, c.id, tarefa),
                None => self.abrir_configuracoes(projeto, nome),
            }
        }
        let lista = self.lista(projeto).to_vec();
        let mut acao: Option<(&str, i64)> = None;
        egui::Popup::menu(&menu).show(|ui| {
            ui.set_min_width(280.0);
            for c in &lista {
                let marcada = escolhida.as_ref().is_some_and(|e| e.id == c.id);
                let texto = if marcada { format!("✓ {}", c.campos.nome) } else { c.campos.nome.clone() };
                if tema::opcao_menu_com(ui, &texto, estado(c).as_deref(), pode) {
                    acao = Some(("rodar", c.id));
                    ui.close();
                }
            }
            if let Some(c) = escolhida.as_ref() {
                ui.add_space(6.0);
                if c.rodando && tema::opcao_menu_com(ui, &format!("Parar {}", cortar(&c.campos.nome)), Some("Ctrl+F2"), pode) {
                    acao = Some(("parar", c.id));
                    ui.close();
                }
                if (c.rodando || c.fim.is_some()) && tema::opcao_menu_com(ui, "Ver a saída", None, true) {
                    acao = Some(("ver", c.id));
                    ui.close();
                }
            }
            ui.add_space(6.0);
            if tema::opcao_menu_com(ui, "Configurações de execução…", None, pode) {
                acao = Some(("configurar", 0));
                ui.close();
            }
        });
        match acao {
            Some(("rodar", id)) => aviso = self.rodar(&ctx, bytes, id, tarefa),
            Some(("parar", id)) => aviso = self.parar(id),
            Some(("ver", id)) => self.mostrar_no_painel(&ctx, bytes, id),
            Some(("configurar", _)) => self.abrir_configuracoes(projeto, nome),
            _ => {}
        }
        let _ = p;
        aviso
    }

    /// O painel de baixo: o nome, o estado, Parar ou Rodar de novo e fechar;
    /// embaixo, o terminal da execução.
    pub fn mostrar_painel(&mut self, ui: &mut egui::Ui, bytes: &Arc<AtomicU64>, pode: bool) -> Option<Aviso> {
        let p = cores();
        let id = self.painel?;
        let ctx = ui.ctx().clone();
        // A lista do projeto pode ter saído do cache (um evento): busca de novo.
        let projeto = self.projeto_do_painel;
        let Some(c) = self.lista(projeto).iter().find(|c| c.id == id).cloned() else {
            self.painel = None;
            return None;
        };
        let mut aviso = None;
        let mut fechar = false;
        ui.horizontal(|ui| {
            tema::play(ui.painter(), ui.cursor().min + egui::vec2(8.0, 12.0), 5.0, if c.rodando { p.ok } else { p.suave });
            ui.add_space(20.0);
            ui.label(tema::texto_forte(&c.campos.nome, 14.0).color(p.texto));
            let (texto, cor) = match (&c.fim, c.rodando) {
                (_, true) => ("rodando".to_string(), p.ok),
                (Some(f), false) if !f.parada && f.codigo != 0 => (format!("saiu com código {} às {}", f.codigo, f.hora), p.erro),
                (Some(_), false) => (estado(&c).unwrap_or_default(), p.suave),
                (None, false) => (String::new(), p.suave),
            };
            ui.label(RichText::new(texto).color(cor).size(12.5));
            if !c.onde.is_empty() {
                ui.label(RichText::new(&c.onde).color(p.suave).size(12.0).monospace());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if tema::botao_icone(ui, tema::Icone::Fechar, 28.0).on_hover_text("Esconder o painel (o programa continua rodando)").clicked() {
                    fechar = true;
                }
                ui.add_space(6.0);
                if c.rodando {
                    if tema::botao_secundario_com(ui, "Parar", pode).on_hover_text("Ctrl+F2").clicked() {
                        aviso = self.parar(id);
                    }
                    ui.add_space(6.0);
                }
                let rotulo = if c.rodando || c.fim.is_some() { "Rodar de novo" } else { "Rodar" };
                if tema::botao_secundario_com(ui, rotulo, pode).on_hover_text("Para o que está rodando e começa de novo").clicked() {
                    // Na mesma pasta da última vez (a da tarefa ou a do projeto).
                    aviso = self.rodar(&ctx, bytes, id, c.tarefa_id);
                }
            });
        });
        ui.add_space(6.0);
        if fechar {
            self.painel = None;
            return aviso;
        }
        if let Some(t) = self.terminais.get_mut(&id) {
            t.mostrar(ui, 13.0, true);
            self.tamanho = Some(t.tamanho());
        }
        aviso
    }

    /// A janela de configurações, se aberta.
    pub fn mostrar_dialogo(&mut self, ctx: &egui::Context) {
        let Some(d) = self.dialogo.as_mut() else { return };
        let projeto = d.projeto;
        let lista = self.comandos.get(&projeto).cloned().unwrap_or_default();
        let mut fechar = false;
        let mut recarregar = false;
        let p = cores();
        egui::Modal::new(egui::Id::new("dialogo-play"))
            .frame(tema::moldura_janela())
            .backdrop_color(egui::Color32::from_black_alpha(if tema::claro() { 60 } else { 140 }))
            .show(ctx, |ui| {
                ui.set_width(620.0);
                tema::cabecalho(
                    ui,
                    "Configurações de execução",
                    &format!(
                        "Em {}. Cada uma roda pelo shell, na pasta do projeto (ou na da tarefa, quando você roda de dentro dela). A saída aparece num painel embaixo.",
                        d.nome_projeto
                    ),
                );
                ui.add_space(14.0);
                let altura = (ctx.content_rect().height() - 300.0).max(200.0);
                egui::ScrollArea::vertical().max_height(altura).auto_shrink([false, true]).show(ui, |ui| {
                    if let Some((id, e)) = d.editando.as_mut() {
                        let titulo = if *id == 0 { "Nova configuração" } else { "Editar configuração" };
                        ui.label(tema::texto_forte(titulo, 14.0).color(p.texto));
                        ui.add_space(8.0);
                        tema::campo(ui, "Nome", &mut e.nome, "api");
                        ui.add_space(8.0);
                        ui.label(RichText::new("Comando").color(p.suave).size(12.5));
                        ui.add_space(2.0);
                        tema::campo_multilinha_com(ui, &mut e.comando, 2, 120.0, egui::Id::new("play-comando"), false, egui::FontId::monospace(13.0));
                        ui.add_space(8.0);
                        tema::campo(ui, "Pasta (relativa ao projeto; vazia é a raiz)", &mut e.pasta, "mercurio-api");
                        ui.add_space(8.0);
                        ui.label(RichText::new("Variáveis de ambiente (uma por linha, NOME=valor)").color(p.suave).size(12.5));
                        ui.add_space(2.0);
                        tema::campo_multilinha_com(ui, &mut e.ambiente, 3, 160.0, egui::Id::new("play-ambiente"), false, egui::FontId::monospace(13.0));
                        ui.add_space(10.0);
                        let (mut salvar, mut cancelar) = (false, false);
                        ui.horizontal(|ui| {
                            cancelar = tema::botao_secundario(ui, "Cancelar").clicked();
                            salvar = tema::botao_principal(ui, "Salvar", !e.nome.trim().is_empty() && !e.comando.trim().is_empty()).clicked();
                        });
                        if salvar {
                            let resultado = e.campos().and_then(|campos| {
                                if *id == 0 { api::criar_comando(projeto, &campos, "voce").map(|_| ()) } else { api::editar_comando(*id, &campos).map(|_| ()) }
                            });
                            match resultado {
                                Ok(()) => {
                                    d.editando = None;
                                    d.erro = None;
                                    recarregar = true;
                                }
                                Err(e) => d.erro = Some(e),
                            }
                        }
                        if cancelar {
                            d.editando = None;
                            d.erro = None;
                        }
                        if let Some(e) = &d.erro {
                            ui.add_space(6.0);
                            ui.label(RichText::new(e).color(p.erro).size(12.5));
                        }
                        ui.add_space(16.0);
                    }

                    // As gravadas.
                    if lista.is_empty() {
                        ui.label(RichText::new("Nenhuma configuração ainda.").color(p.suave).size(13.0));
                    }
                    let mut editar = None;
                    let mut remover = None;
                    for c in &lista {
                        linha(ui, &c.campos, if c.origem == "intellij" { "IntelliJ" } else { "" }, |ui| {
                            if tema::botao_secundario(ui, "Remover").clicked() {
                                remover = Some(c.id);
                            }
                            if tema::botao_secundario(ui, "Editar").clicked() {
                                editar = Some(c.clone());
                            }
                        });
                    }
                    if let Some(c) = editar {
                        d.editando = Some((c.id, Edicao::de(&c.campos)));
                    }
                    if let Some(id) = remover {
                        match api::remover_comando(id) {
                            Ok(()) => recarregar = true,
                            Err(e) => d.erro = Some(e),
                        }
                    }
                    if d.editando.is_none() {
                        ui.add_space(8.0);
                        if tema::botao_secundario(ui, "+ Nova configuração").clicked() {
                            d.editando = Some((0, Edicao::default()));
                        }
                    }

                    // As achadas no IntelliJ e nos arquivos do projeto.
                    match &d.sugestoes {
                        Ok(s) if !s.sugestoes.is_empty() || !s.nao_suportadas.is_empty() => {
                            ui.add_space(20.0);
                            ui.horizontal(|ui| {
                                ui.label(tema::texto_forte("Encontradas no projeto", 14.0).color(p.texto));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if !s.sugestoes.is_empty() && tema::botao_secundario(ui, "Adicionar todas").clicked() {
                                        let mut erros = Vec::new();
                                        for sug in &s.sugestoes {
                                            if let Err(e) = api::criar_comando(projeto, &sug.campos, &sug.origem) {
                                                erros.push(format!("{}: {e}", sug.campos.nome));
                                            }
                                        }
                                        d.erro = (!erros.is_empty()).then(|| erros.join("\n"));
                                        recarregar = true;
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            let mut adicionar = None;
                            for (i, sug) in s.sugestoes.iter().enumerate() {
                                linha(ui, &sug.campos, &sug.fonte, |ui| {
                                    if tema::botao_secundario(ui, "Adicionar").clicked() {
                                        adicionar = Some(i);
                                    }
                                });
                            }
                            if let Some(i) = adicionar {
                                let sug = &s.sugestoes[i];
                                match api::criar_comando(projeto, &sug.campos, &sug.origem) {
                                    Ok(_) => recarregar = true,
                                    Err(e) => d.erro = Some(e),
                                }
                            }
                            if !s.nao_suportadas.is_empty() {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(format!("O IntelliJ tem também, de tipos que a Colmeia ainda não roda: {}.", s.nao_suportadas.join(", ")))
                                        .color(p.suave)
                                        .size(12.5),
                                );
                            }
                        }
                        Err(e) => {
                            ui.add_space(12.0);
                            ui.label(RichText::new(format!("Não consegui procurar no projeto: {e}")).color(p.erro).size(12.5));
                        }
                        _ => {}
                    }
                    if d.editando.is_none()
                        && let Some(e) = &d.erro
                    {
                        ui.add_space(8.0);
                        ui.label(RichText::new(e).color(p.erro).size(12.5));
                    }
                });
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tema::botao_principal(ui, "Fechar", true).clicked() {
                        fechar = true;
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    fechar = true;
                }
            });
        if recarregar {
            self.comandos.remove(&projeto);
            self.lista(projeto);
            if let Some(d) = self.dialogo.as_mut() {
                d.sugestoes = api::sugestoes_de_comandos(projeto);
            }
        }
        if fechar {
            self.dialogo = None;
        }
    }

    pub fn dialogo_aberto(&self) -> bool {
        self.dialogo.is_some()
    }
}

/// Uma configuração numa lista: nome e origem em cima, o comando embaixo, os
/// botões à direita.
fn linha(ui: &mut egui::Ui, c: &CamposComando, fonte: &str, botoes: impl FnOnce(&mut egui::Ui)) {
    let p = cores();
    egui::Frame::new()
        .fill(p.superficie)
        .stroke(egui::Stroke::new(1.0, p.borda))
        .corner_radius(egui::CornerRadius::same(tema::RAIO_CONTROLE))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(ui.available_width() - 190.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(tema::texto_forte(&c.nome, 13.5).color(p.texto)).truncate());
                        if !fonte.is_empty() {
                            ui.label(RichText::new(fonte).color(p.suave).size(11.5));
                        }
                    });
                    let mut onde = c.comando.replace('\n', " ⏎ ");
                    if !c.pasta.is_empty() {
                        onde = format!("{}  ·  {onde}", c.pasta);
                    }
                    ui.add(egui::Label::new(RichText::new(onde).color(p.suave).size(12.0).monospace()).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), botoes);
            });
        });
    ui.add_space(6.0);
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn variaveis_uma_por_linha() {
        let e =
            Edicao { nome: " api ".into(), comando: "go run .".into(), pasta: "".into(), ambiente: "PORTA=8081\n# comentário\n\nURL=http://x?a=b\n".into() };
        let c = e.campos().unwrap();
        assert_eq!(c.nome, "api");
        assert_eq!(c.ambiente, vec![Variavel { nome: "PORTA".into(), valor: "8081".into() }, Variavel { nome: "URL".into(), valor: "http://x?a=b".into() }]);
        assert!(Edicao { ambiente: "SEM_IGUAL".into(), ..Edicao::default() }.campos().is_err());
        assert_eq!(Edicao::de(&c).ambiente, "PORTA=8081\nURL=http://x?a=b");
    }

    #[test]
    fn estado_curto() {
        let mut c = Comando::default();
        assert_eq!(estado(&c), None);
        c.fim = Some(api::FimExecucao { codigo: 2, parada: false, hora: "14:32".into() });
        assert_eq!(estado(&c).as_deref(), Some("código 2 às 14:32"));
        c.rodando = true;
        assert_eq!(estado(&c).as_deref(), Some("rodando"));
        assert_eq!(cortar("a".repeat(40).as_str()).chars().count(), NOME_NO_BOTAO);
    }
}
