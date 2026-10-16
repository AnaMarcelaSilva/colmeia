//! O painel da tarefa, em abas: Terminal (em destaque ao abrir), Arquivos,
//! Lousa, Navegador e Play, Descrição e Linha do tempo.
//!
//! No Terminal, os agentes viram fichas em cima do terminal em foco (com o
//! anel de contexto quando a statusLine mandou o uso); a ficha escolhida se
//! funde ao terminal e a caixa de mensagem fica no pé dele. O mosaico (os
//! outros terminais em miniatura ao lado) continua a um clique.
//!
//! Trocar de aba não muda o tamanho de nenhum terminal: fora da aba Terminal
//! ele só não é desenhado, e volta do mesmo tamanho. Só o terminal em foco,
//! na aba Terminal, é tempo real; as outras abas só redesenham com entrada
//! ou resposta.

use eframe::egui::{self, CornerRadius, FontId, Key, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

use super::{CLARAO, Colmeia, Pedido, RespostaNavegador, TITULO_MINIMO, Tela, cartao_de_fim};
use crate::compositor::Compositor;
use crate::dados::AgenteTela;
use crate::tema::{self, EstadoVisual, cores, texto_forte};
use crate::{api, arquivos, descricao, gaveta, sistema, terminal};

/// Altura da fileira das fichas dos agentes.
const ALTURA_FICHAS: f32 = 52.0;
/// Vão entre o terminal e a caixa de mensagem, dentro do cartão.
const MARGEM_CARTAO: f32 = 10.0;

/// As abas da tarefa, na ordem da barra (Ctrl+1…6).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Aba {
    #[default]
    Terminal,
    Arquivos,
    Lousa,
    Navegador,
    Descricao,
    Linha,
}

impl Aba {
    pub const TODAS: [Aba; 6] = [Aba::Terminal, Aba::Arquivos, Aba::Lousa, Aba::Navegador, Aba::Descricao, Aba::Linha];

    fn indice(self) -> usize {
        Aba::TODAS.iter().position(|a| *a == self).unwrap_or(0)
    }

    fn nome(self) -> &'static str {
        match self {
            Aba::Terminal => "Terminal",
            Aba::Arquivos => "Arquivos",
            Aba::Lousa => "Lousa",
            Aba::Navegador => "Navegador e Play",
            Aba::Descricao => "Descrição",
            Aba::Linha => "Linha do tempo",
        }
    }

    fn icone(self) -> tema::Icone {
        match self {
            Aba::Terminal => tema::Icone::Terminal,
            Aba::Arquivos => tema::Icone::Pasta,
            Aba::Lousa => tema::Icone::Lousa,
            Aba::Navegador => tema::Icone::Globo,
            Aba::Descricao => tema::Icone::Texto,
            Aba::Linha => tema::Icone::Relogio,
        }
    }
}

/// A borda do cartão do terminal em foco (e da ficha dele): acompanha o
/// estado; com o teclado na caixa de mensagem, fica só marcada de leve.
fn borda_do_cartao(visual: EstadoVisual, compositor_com_foco: bool) -> Stroke {
    let p = cores();
    match visual {
        EstadoVisual::PedeAprovacao | EstadoVisual::PedeConsulta | EstadoVisual::SuaVez => Stroke::new(1.5, p.alerta),
        EstadoVisual::Erro => Stroke::new(1.5, p.erro),
        _ if compositor_com_foco => Stroke::new(1.0, p.destaque.gamma_multiply(0.45)),
        _ => Stroke::new(1.5, p.destaque.gamma_multiply(0.8)),
    }
}

/// Onde cada ficha vai na fileira: a largura de cada uma, até 280.
fn larguras_das_fichas(disponivel: f32, n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    ((disponivel - 4.0 * (n as f32 - 1.0)) / n as f32).clamp(120.0, 280.0)
}

impl Colmeia {
    /// A aba da tarefa na tela é `aba`.
    pub(super) fn na_aba(&self, id: i64, aba: Aba) -> bool {
        self.tarefa_na_tela == Some(id) && self.aba == aba
    }

    /// Troca a aba da tarefa: o que a aba nova precisa é pedido agora (o
    /// que mudou desde a última visita), não por tempo.
    pub(super) fn mudar_aba(&mut self, id: i64, aba: Aba, ctx: &egui::Context) {
        if self.tarefa_na_tela == Some(id) && self.aba == aba {
            return;
        }
        self.tarefa_na_tela = Some(id);
        self.aba = aba;
        match aba {
            Aba::Terminal => {
                if let Tela::Tarefa { foco, .. } = self.tela
                    && let Some(t) = self.terminais.get(&foco)
                {
                    t.focar();
                }
            }
            Aba::Lousa => {
                self.novidade_na_lousa.remove(&id);
            }
            Aba::Arquivos => match self.abas_arquivos.get_mut(&id) {
                Some(a) => a.ao_abrir(ctx),
                None => {
                    self.abas_arquivos.insert(id, arquivos::AbaArquivos::nova(ctx, id));
                }
            },
            Aba::Descricao => {
                self.novidade_na_descricao.remove(&id);
                match self.descricoes.get_mut(&id) {
                    Some(d) => d.pedir(ctx),
                    None => {
                        self.descricoes.insert(id, descricao::Descricao::nova(ctx, id));
                    }
                }
            }
            Aba::Navegador | Aba::Linha => {}
        }
    }

    /// Chegou uma nota da tarefa: a Descrição aberta busca de novo; fora da
    /// tela, a do agente vira o ponto na aba.
    pub(super) fn nota_mudou(&mut self, tarefa: i64, agente: i64, ctx: &egui::Context) {
        let visivel = matches!(self.tela, Tela::Tarefa { id, .. } if id == tarefa) && self.na_aba(tarefa, Aba::Descricao);
        if visivel {
            if let Some(d) = self.descricoes.get_mut(&tarefa) {
                d.pedir(ctx);
            }
        } else if agente != 0 {
            self.novidade_na_descricao.insert(tarefa);
        }
    }

    /// Ctrl+Shift+Q e Ctrl+Shift+E: a aba vai e volta do Terminal.
    pub(super) fn alternar_aba(&mut self, id: i64, aba: Aba, ctx: &egui::Context) {
        let nova = if self.na_aba(id, aba) { Aba::Terminal } else { aba };
        self.mudar_aba(id, nova, ctx);
    }

    pub(super) fn painel_tarefa(&mut self, ui: &mut egui::Ui, id: i64, foco: i64, agora: f64) {
        let Some(tarefa) = self.modelo.tarefas.iter().find(|t| t.id == id) else {
            self.tela = Tela::Quadro;
            return;
        };
        // Abrir a tarefa mostra o terminal (só a linha do tempo, pela
        // lousa, abre direto na Lousa).
        if self.tarefa_na_tela != Some(id) {
            self.tarefa_na_tela = Some(id);
            self.aba = Aba::Terminal;
        }
        let agentes = tarefa.agentes.clone();
        let pasta = tarefa.pasta.clone();
        let projeto = tarefa.projeto_id;
        let (voltar, novo_agente, abrir_editor) = self.cabecalho_tarefa(ui, id);
        ui.add_space(4.0);
        if let Some(aba) = self.barra_de_abas(ui, id, projeto, &agentes) {
            self.mudar_aba(id, aba, ui.ctx());
        }
        ui.add_space(10.0);
        if abrir_editor
            && let Some((nome, comando)) = self.editor
            && let Err(e) = sistema::abrir_com(comando, &pasta)
        {
            self.erro(format!("Não consegui abrir o {nome}: {e}"), agora);
        }
        if novo_agente {
            self.adicionar_agente(id, false);
        }
        if voltar || ui.input(|i| i.key_pressed(Key::Escape) && i.modifiers.ctrl) {
            self.tela = match self.voltar_para_lousa.take() {
                Some(ws) => Tela::Lousa(ws),
                None => Tela::Quadro,
            };
            return;
        }
        match self.aba {
            Aba::Terminal => {
                self.pedidos_de_consulta(ui, id, &agentes, agora);
                self.aba_terminal(ui, id, foco, &agentes, &pasta, agora);
            }
            Aba::Arquivos => self.aba_arquivos(ui, id, &pasta, agora),
            Aba::Lousa => {
                let area = ui.available_rect_before_wrap();
                let dono = api::DonoLousa { workspace_id: 0, tarefa_id: id };
                self.ancora_aviso = Some(area.center_bottom() - vec2(0.0, 16.0));
                let acoes = self.mostrar_lousa(ui, area, dono, agora);
                self.tratar_lousa(dono, acoes, ui.ctx(), agora);
            }
            Aba::Navegador => self.aba_navegador(ui, id, projeto, agora),
            Aba::Descricao => {
                let ctx = ui.ctx().clone();
                self.descricoes.entry(id).or_insert_with(|| descricao::Descricao::nova(&ctx, id)).mostrar(ui);
            }
            Aba::Linha => {
                let perfil = self.perfil.as_ref().map_or(0, |p| p.id);
                self.registro.conectado = self.pode_mudar();
                for acao in self.registro.mostrar_da_tarefa(ui, perfil, id) {
                    match acao {
                        crate::registro::Acao::Avisar(tipo, texto) => self.avisar(tipo, texto, agora),
                        crate::registro::Acao::AbrirTarefa { tarefa, .. } if tarefa != id => self.abrir_tarefa(tarefa, None),
                        _ => {}
                    }
                }
            }
        }
    }

    /// O cabeçalho: voltar, título, número, branch e estado; à direita
    /// "Abrir no IntelliJ" e "+ Agente". Devolve (voltar, novo agente, abrir no editor).
    fn cabecalho_tarefa(&mut self, ui: &mut egui::Ui, id: i64) -> (bool, bool, bool) {
        let Some(tarefa) = self.modelo.tarefas.iter().find(|t| t.id == id) else { return (false, false, false) };
        let p = cores();
        let pode = self.pode_mudar();
        let (mut voltar, mut novo_agente, mut abrir_editor) = (false, false, false);
        let volta_para_lousa = self.voltar_para_lousa.is_some();
        let demo = self.demo;
        let editor = self.editor;
        ui.horizontal(|ui| {
            voltar = if volta_para_lousa {
                tema::botao_secundario(ui, "‹ Lousa").on_hover_text("Voltar à lousa (Ctrl+Esc)").clicked()
            } else {
                tema::botao_secundario(ui, "‹ Quadro").on_hover_text("Voltar ao quadro (Ctrl+Esc)").clicked()
            };
            ui.add_space(8.0);
            // O título é o que se corta: os botões da direita, o número, a
            // branch e a etiqueta de estado nunca. O título inteiro fica na dica.
            let espaco = ui.spacing().item_spacing.x;
            let corpo = egui::TextStyle::Body.resolve(ui.style());
            let pintor = ui.painter().clone();
            let medir = |t: &str, fonte: egui::FontId| pintor.layout_no_wrap(t.to_owned(), fonte, p.texto).size().x;
            let mut reservado = 2.0;
            let mut reservado_editor = 0.0;
            if !demo {
                reservado += medir("+ Agente", tema::forte(13.5)) + 32.0 + 6.0 + espaco;
                if let Some((nome, _)) = editor {
                    reservado_editor = medir(&format!("Abrir no {nome}"), egui::FontId::proportional(13.0)) + 28.0 + 6.0 + espaco;
                }
            }
            reservado += medir(&format!("#{}", tarefa.id), corpo.clone()) + espaco;
            let branch = if tarefa.branch.is_empty() { "pasta" } else { tarefa.branch.as_str() };
            reservado += medir(branch, egui::FontId::monospace(11.0)) + 12.0 + espaco;
            if tarefa.em_copia && !demo {
                reservado += medir("cópia isolada", corpo.clone()) + espaco;
            }
            reservado += medir(tema::rotulo_coluna(tarefa.coluna), tema::fonte_etiqueta()) + 12.0 + espaco;
            if tarefa.erro.is_some() {
                reservado += medir("Erro", tema::fonte_etiqueta()) + 12.0 + espaco;
            }
            // Abaixo de 160 para o título, "Abrir no IntelliJ" vira "IntelliJ".
            let editor_curto = editor.is_some() && ui.available_width() - reservado - reservado_editor < TITULO_MINIMO;
            if let Some((nome, _)) = editor
                && editor_curto
            {
                reservado_editor = medir(nome, egui::FontId::proportional(13.0)) + 28.0 + 6.0 + espaco;
            }
            reservado += reservado_editor;
            let largura_titulo = (ui.available_width() - reservado).max(80.0);
            ui.allocate_ui(vec2(largura_titulo, 24.0), |ui| {
                ui.add(egui::Label::new(texto_forte(&tarefa.titulo, 17.0).color(p.texto)).truncate());
            });
            ui.label(RichText::new(format!("#{}", tarefa.id)).color(p.suave));
            if tarefa.branch.is_empty() {
                tema::etiqueta_ui(ui, "pasta", egui::FontId::monospace(11.0), p.suave);
            } else {
                tema::etiqueta_ui(ui, &tarefa.branch, egui::FontId::monospace(11.0), p.destaque);
            }
            if tarefa.em_copia && !demo {
                ui.label(RichText::new("cópia isolada").color(p.suave)).on_hover_text(&tarefa.pasta);
            }
            tema::etiqueta_ui(ui, tema::rotulo_coluna(tarefa.coluna), tema::fonte_etiqueta(), tema::cor_coluna(tarefa.coluna));
            if let Some(erro) = &tarefa.erro {
                tema::etiqueta_ui(ui, "Erro", tema::fonte_etiqueta(), p.erro).on_hover_text(erro);
            }
            if demo {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                novo_agente = tema::botao_principal(ui, "+ Agente", pode).on_hover_text("Abrir um agente (Claude Code, Codex…) nesta tarefa").clicked();
                if let Some((nome, _)) = editor {
                    ui.add_space(6.0);
                    let rotulo = if editor_curto { nome.to_string() } else { format!("Abrir no {nome}") };
                    abrir_editor = tema::botao_secundario(ui, &rotulo).on_hover_text(format!("Abrir no {nome}: {}", tarefa.pasta)).clicked();
                }
            });
        });
        (voltar, novo_agente, abrir_editor)
    }

    /// As abas, com o número de agentes (âmbar quando um espera você), o
    /// número de arquivos mudados e os pontos: novidade na lousa e na
    /// descrição, navegador ou Play abertos. Devolve a aba clicada.
    fn barra_de_abas(&mut self, ui: &mut egui::Ui, id: i64, projeto: i64, agentes: &[AgenteTela]) -> Option<Aba> {
        let p = cores();
        let espera = agentes.iter().any(|a| a.visual().espera_voce());
        let mudaram = self.abas_arquivos.get(&id).and_then(|a| a.quantas_mudaram()).filter(|n| *n > 0);
        let aberto = !self.demo && (self.modelo.navegadores.contains(&id) || self.play.rodando(projeto));
        let abas: Vec<tema::Aba> = Aba::TODAS
            .iter()
            .map(|a| {
                let (numero, ponto) = match a {
                    Aba::Terminal => ((!agentes.is_empty()).then_some((agentes.len(), espera)), None),
                    Aba::Arquivos => (mudaram.map(|n| (n, false)), None),
                    Aba::Lousa => (None, self.novidade_na_lousa.contains(&id).then_some(p.destaque)),
                    Aba::Navegador => (None, aberto.then_some(p.ok)),
                    Aba::Descricao => (None, self.novidade_na_descricao.contains(&id).then_some(p.destaque)),
                    Aba::Linha => (None, None),
                };
                tema::Aba { icone: a.icone(), nome: a.nome(), numero, ponto }
            })
            .collect();
        let ate = ui.max_rect().right();
        tema::abas(ui, &abas, self.aba.indice(), ate).map(|i| Aba::TODAS[i])
    }

    /// A aba Terminal: as fichas dos agentes, o terminal em foco com a caixa
    /// de mensagem no pé e, no mosaico, os outros ao lado.
    fn aba_terminal(&mut self, ui: &mut egui::Ui, id: i64, foco: i64, agentes: &[AgenteTela], pasta: &str, agora: f64) {
        let area = ui.available_rect_before_wrap();
        if agentes.is_empty() {
            // Onde o terminal do primeiro agente vai aparecer, para ele já nascer do tamanho certo.
            let altura_msg = self.compositor.altura() + MARGEM_CARTAO;
            let terminal = Rect::from_min_max(area.min + vec2(MARGEM_CARTAO, ALTURA_FICHAS + 6.0), area.max - vec2(MARGEM_CARTAO, altura_msg + 6.0));
            terminal::estimar_em_foco(ui, terminal, 13.0);
            self.sem_agentes(ui, id, pasta);
            return;
        }
        let foco = if agentes.iter().any(|a| a.id == foco) { foco } else { agentes[0].id };
        self.topo_terminal = Some(area.top());
        let em_foco = agentes.iter().find(|a| a.id == foco).cloned().expect("o foco é um dos agentes");
        let outros: Vec<AgenteTela> = if self.mosaico { agentes.iter().filter(|a| a.id != foco).cloned().collect() } else { Vec::new() };
        let largura_lateral = if outros.is_empty() { 0.0 } else { (area.width() * 0.3).max(280.0) };
        let coluna = Rect::from_min_max(
            pos2(area.left(), area.top() + ALTURA_FICHAS),
            pos2(area.right() - largura_lateral - if outros.is_empty() { 0.0 } else { 10.0 }, area.bottom()),
        );
        let faixa = Rect::from_min_max(area.min, pos2(area.right(), coluna.top()));
        let visual = if self.demo { EstadoVisual::Trabalhando } else { em_foco.visual() };
        let borda = borda_do_cartao(visual, Compositor::com_foco(ui.ctx()));
        let mut pedidos = self.fichas(ui, faixa, coluna, borda.color, agentes, foco);

        // O cartão do terminal: o terminal em cima e a caixa de mensagem no pé.
        let altura_msg = self.compositor.altura();
        let terminal_rect = Rect::from_min_max(coluna.min + vec2(8.0, 8.0), pos2(coluna.right() - 8.0, coluna.bottom() - altura_msg - 2.0 * MARGEM_CARTAO));
        pedidos.push(self.terminal_em_foco(ui, coluna, terminal_rect, borda, &em_foco, agora));
        // O aviso do rodapé fica sobre o terminal, sem cobrir a caixa de mensagem.
        self.ancora_aviso = Some(terminal_rect.center_bottom() - vec2(0.0, 16.0));
        let caixa_mensagem =
            Rect::from_min_max(pos2(coluna.left() + MARGEM_CARTAO, terminal_rect.bottom() + MARGEM_CARTAO), coluna.max - vec2(MARGEM_CARTAO, 2.0));
        // Com um diálogo ou caixa por cima, o Enter é de quem está por cima.
        let teclado_livre = self.dialogo.is_none()
            && !self.banco.modal_aberto()
            && self.caixa_pedido.is_none()
            && self.apresentacao.is_none()
            && self.palco.is_none()
            && self.popover_navegador.is_none();
        if let Some(envio) = self.compositor.mostrar(ui, caixa_mensagem, agentes, foco, id, teclado_livre) {
            for agente in envio.destinos {
                if let Some(t) = self.terminais.get(&agente) {
                    t.enviar(&envio.texto);
                }
            }
        }

        // O mosaico: os outros agentes em miniatura, à direita.
        let altura = if outros.is_empty() { 0.0 } else { (coluna.height() - 10.0 * (outros.len() as f32 - 1.0)) / outros.len() as f32 };
        for (n, agente) in outros.iter().enumerate() {
            let min = pos2(area.right() - largura_lateral, coluna.top() + n as f32 * (altura + 10.0));
            let caixa = Rect::from_min_size(min, vec2(largura_lateral, altura));
            pedidos.push(self.caixa_terminal(ui, caixa, agente));
        }
        self.tratar_pedidos(pedidos.into_iter().flatten().collect(), id, agentes, ui.ctx(), agora);
    }

    fn tratar_pedidos(&mut self, pedidos: Vec<Pedido>, id: i64, agentes: &[AgenteTela], ctx: &egui::Context, agora: f64) {
        for pedido in pedidos {
            match pedido {
                Pedido::Focar(agente) => {
                    self.tela = Tela::Tarefa { id, foco: agente };
                    if let Some(t) = self.terminais.get(&agente) {
                        t.focar();
                    }
                }
                Pedido::Iniciar(agente) => self.iniciar_agente(agente, ctx, agora),
                Pedido::Remover(agente) => self.remover_agente(agente, agora),
                Pedido::Capturar(agente) => self.capturar(agente, id),
                Pedido::LimparHistorico(agente, guardadas) => {
                    let nome = agentes.iter().find(|a| a.id == agente).map(|a| a.nome_com_papel()).unwrap_or_default();
                    self.dialogo = Some(super::Dialogo::LimparHistorico { agente, nome, guardadas, erro: None });
                }
            }
        }
    }

    /// As fichas dos agentes em cima do terminal: anel de contexto (quando há
    /// uso), estado, nome, papel e a última linha. A escolhida se funde ao
    /// cartão do terminal, como a aba de uma pasta; à direita, o mosaico.
    fn fichas(&mut self, ui: &mut egui::Ui, faixa: Rect, cartao: Rect, borda: egui::Color32, agentes: &[AgenteTela], foco: i64) -> Vec<Option<Pedido>> {
        let p = cores();
        let demo = self.demo;
        let pode = self.pode_mudar();
        let mut pedidos = Vec::new();
        // O botão do mosaico, à direita da fileira (só com mais de um agente).
        let mut fim = faixa.right();
        if agentes.len() > 1 {
            let largura = ui.painter().layout_no_wrap("Mosaico".into(), tema::forte(13.0), p.texto).size().x + 28.0;
            let r = Rect::from_min_size(pos2(faixa.right() - largura, faixa.top() + 6.0), vec2(largura, 32.0));
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(r));
            if tema::chip_alternar(&mut filho, "Mosaico", self.mosaico).on_hover_text("Ver os outros terminais ao lado do que está em foco").clicked() {
                self.mosaico = !self.mosaico;
            }
            fim = r.left() - 12.0;
        }
        // No mosaico, as fichas ficam em cima do terminal em foco (a escolhida se funde a ele).
        let fim = fim.min(cartao.right());
        let largura = larguras_das_fichas(fim - faixa.left() - 6.0, agentes.len());
        for (n, a) in agentes.iter().enumerate() {
            let rect = Rect::from_min_size(pos2(faixa.left() + 6.0 + n as f32 * (largura + 4.0), faixa.top() + 4.0), vec2(largura, faixa.height() - 4.0));
            let ativa = a.id == foco;
            let resposta = ui.interact(rect, egui::Id::new(("ficha", a.id)), Sense::click());
            let visual = if demo { EstadoVisual::Trabalhando } else { a.visual() };
            if ativa {
                let cantos = CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 };
                ui.painter().rect(rect, cantos, p.terminal_fundo, Stroke::new(1.5, borda), StrokeKind::Inside);
                // A emenda com o cartão some: a ficha e o terminal são uma peça só.
                ui.painter().rect_filled(
                    Rect::from_min_max(pos2(rect.left() + 1.0, cartao.top() - 2.0), pos2(rect.right() - 1.0, cartao.top() + 1.5)),
                    0.0,
                    p.terminal_fundo,
                );
                let traco = Rect::from_min_size(pos2(rect.left() + 14.0, rect.top()), vec2(rect.width() - 28.0, 2.0));
                ui.painter().rect_filled(traco, CornerRadius::same(1), p.destaque);
            } else {
                let fundo = tema::cor_animada(ui, resposta.id.with("mouse"), resposta.hovered(), p.realce.gamma_multiply(0.0), p.realce);
                ui.painter().rect_filled(rect.shrink2(vec2(0.0, 2.0)), CornerRadius::same(12), fundo);
            }
            // O anel com o contexto usado (sem dado de uso, só o ponto do estado).
            let centro = pos2(rect.left() + 24.0, rect.center().y);
            match self.modelo.uso.contextos.get(&a.id) {
                Some(uso) => {
                    let parte = uso.porcentagem / 100.0;
                    let cor = if parte >= 0.8 { p.alerta } else { p.destaque };
                    tema::anel(ui.painter(), centro, 13.0, 3.0, parte, cor);
                    let texto = format!("{:.0}", uso.porcentagem);
                    ui.painter().text(centro, egui::Align2::CENTER_CENTER, texto, tema::forte(9.5), p.texto);
                    tema::ponto(ui.painter(), centro + vec2(10.0, -10.0), 3.5, visual);
                }
                None => tema::ponto(ui.painter(), centro, 4.5, visual),
            }
            let x = rect.left() + 46.0;
            let largura_texto = rect.right() - x - if ativa && !demo { 34.0 } else { 10.0 };
            let g = tema::cortar(ui.painter(), &a.nome(), egui::TextFormat::simple(tema::forte(13.0), p.texto), largura_texto, 1, true);
            let w_nome = g.size().x;
            ui.painter().galley(pos2(x, rect.top() + 9.0), g, p.texto);
            crate::registro::texto_cortado(
                ui.painter(),
                pos2(x + w_nome + 6.0, rect.top() + 17.0),
                &a.papel,
                FontId::proportional(11.5),
                p.suave,
                largura_texto - w_nome - 6.0,
            );
            // A segunda linha: quem espera você diz o porquê; os outros, a última linha do terminal.
            let (linha, cor) = if visual.espera_voce() || !a.ativo {
                (a.texto_estado(), if visual.espera_voce() { p.alerta } else { visual.cor() })
            } else {
                (self.terminais.get(&a.id).map(|t| t.ultima_linha()).unwrap_or_default(), p.suave)
            };
            crate::registro::texto_cortado(ui.painter(), pos2(x, rect.top() + 34.0), &linha, FontId::monospace(10.5), cor, largura_texto);
            let dica = format!("{} · {}", a.texto_estado(), if ativa { "em foco" } else { "clique para focar" });
            let resposta = resposta.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(dica);
            if resposta.clicked() && !ativa {
                pedidos.push(Some(Pedido::Focar(a.id)));
            }
            if demo {
                continue;
            }
            // As ações do agente: no ⋯ da ficha em foco e no botão direito de qualquer uma.
            let menu = |ui: &mut egui::Ui, pedido: &mut Option<Pedido>| {
                let pode_capturar = ativa && pode;
                if tema::opcao_menu_com(ui, "Capturar terminal", Some("Ctrl+Shift+S"), pode_capturar) {
                    *pedido = Some(Pedido::Capturar(a.id));
                    ui.close();
                }
                let guardadas = self.compositor.tamanho_historico(a.id);
                if tema::opcao_menu_com(ui, "Limpar histórico de mensagens", None, pode && guardadas > 0) {
                    *pedido = Some(Pedido::LimparHistorico(a.id, guardadas));
                    ui.close();
                }
                if tema::opcao_menu_com(ui, "Parar e remover agente", None, pode) {
                    *pedido = Some(Pedido::Remover(a.id));
                    ui.close();
                }
            };
            let mut pedido = None;
            if ativa {
                let r = Rect::from_center_size(pos2(rect.right() - 20.0, rect.top() + 18.0), vec2(26.0, 26.0));
                let mais = tema::botao_icone_em(ui, r, egui::Id::new(("acoes-agente", a.id)), tema::Icone::Mais).on_hover_text("Ações do agente");
                egui::Popup::menu(&mais).show(|ui| {
                    ui.set_min_width(240.0);
                    menu(ui, &mut pedido);
                });
            }
            resposta.context_menu(|ui| {
                ui.set_min_width(240.0);
                menu(ui, &mut pedido);
            });
            pedidos.push(pedido);
        }
        pedidos
    }

    /// O cartão do terminal em foco (sem cabeçalho: a ficha faz esse papel).
    /// A borda acompanha o estado; um agente parado mostra o cartão de fim.
    fn terminal_em_foco(&mut self, ui: &mut egui::Ui, cartao: Rect, rect: Rect, borda: Stroke, agente: &AgenteTela, agora: f64) -> Option<Pedido> {
        let p = cores();
        let pode = self.pode_mudar();
        let parado = !self.demo && !agente.ativo;
        ui.painter().rect(cartao, CornerRadius::same(14), p.terminal_fundo, borda, StrokeKind::Inside);
        let mut pedido = None;
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        match self.terminais.get_mut(&agente.id) {
            Some(t) => {
                t.mostrar(&mut filho, 13.0, true);
            }
            None => {
                filho.allocate_response(filho.available_size(), Sense::hover());
            }
        }
        if parado && let Some(pd) = cartao_de_fim(&mut filho, rect.center(), agente, true, pode) {
            pedido = Some(pd);
        }
        // Clarão de 0,3 s na borda do terminal capturado: o único redesenho seguido da captura.
        if let Some((capturado, inicio)) = self.clarao
            && capturado == agente.id
        {
            let t = ((agora - inicio) / CLARAO) as f32;
            if t < 1.0 {
                ui.painter().rect_stroke(cartao, CornerRadius::same(14), Stroke::new(3.0, p.texto.gamma_multiply(1.0 - t)), StrokeKind::Inside);
                ui.ctx().request_repaint();
            } else {
                self.clarao = None;
            }
        }
        pedido
    }

    /// Uma miniatura do mosaico: cabeçalho com estado e o terminal com letra menor.
    fn caixa_terminal(&mut self, ui: &mut egui::Ui, caixa: Rect, agente: &AgenteTela) -> Option<Pedido> {
        let p = cores();
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(caixa));
        let mut pedido = None;
        let demo = self.demo;
        let pode = self.pode_mudar();
        let visual = if demo { EstadoVisual::Trabalhando } else { agente.visual() };
        let parado = !demo && !agente.ativo;
        let borda = match visual {
            EstadoVisual::PedeAprovacao | EstadoVisual::PedeConsulta | EstadoVisual::SuaVez => Stroke::new(1.5, p.alerta),
            EstadoVisual::Erro => Stroke::new(1.5, p.erro),
            _ => Stroke::new(1.0, p.borda),
        };
        egui::Frame::new()
            .fill(p.superficie_alta)
            .stroke(borda)
            .corner_radius(CornerRadius::same(tema::RAIO_SUPERFICIE))
            .inner_margin(egui::Margin::symmetric(0, 6))
            .show(&mut filho, |ui| {
                // A moldura soma a margem de 6 em cima e embaixo e a borda.
                ui.set_min_size(caixa.size() - vec2(3.0, 15.0));
                ui.horizontal(|ui| {
                    ui.set_height(28.0);
                    ui.add_space(10.0);
                    let (ponto, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                    tema::ponto(ui.painter(), ponto.center(), 3.5, visual);
                    ui.label(texto_forte(agente.nome(), 13.5).color(p.texto));
                    ui.label(RichText::new(format!("· {}", agente.papel)).color(p.suave));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(6.0);
                        tema::etiqueta_ui(ui, visual.selo(), tema::fonte_etiqueta(), visual.cor())
                            .on_hover_text(format!("{} · clique para focar", agente.texto_estado()));
                    });
                });
                match self.terminais.get_mut(&agente.id) {
                    Some(t) => {
                        if t.mostrar(ui, 10.0, false).clicked() {
                            pedido = Some(Pedido::Focar(agente.id));
                        }
                    }
                    None => {
                        if ui.allocate_response(ui.available_size(), Sense::click()).clicked() {
                            pedido = Some(Pedido::Focar(agente.id));
                        }
                    }
                }
                if parado {
                    let centro = ui.min_rect().center();
                    if let Some(p) = cartao_de_fim(ui, centro, agente, false, pode) {
                        pedido = Some(p);
                    }
                }
            });
        pedido
    }

    /// Tarefa sem agentes: abrir um novo ou retomar uma conversa do Claude Code
    /// começada em outro lugar (no IntelliJ, por exemplo) na mesma pasta.
    fn sem_agentes(&mut self, ui: &mut egui::Ui, id: i64, pasta: &str) {
        let tem_claude = !self.demo && self.opcoes_de_agente().iter().any(|o| o.id == "claude");
        let pode = self.pode_mudar();
        let (mut novo, mut retomar) = (false, false);
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.allocate_ui(vec2(520.0, 0.0), |ui| {
                tema::moldura_janela().show(ui, |ui| {
                    ui.set_width(472.0);
                    let texto = if self.demo {
                        "Na demonstração, os agentes são terminais de teste.".to_string()
                    } else {
                        format!("Para falar com o Claude, adicione um agente: o terminal dele abre aqui, em {pasta}, com a caixa de mensagem embaixo. Se você já conversava com o Claude Code nessa pasta, dá para continuar a mesma conversa.")
                    };
                    tema::cabecalho(ui, "Esta tarefa ainda não tem agentes", &texto);
                    if self.demo {
                        return;
                    }
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        novo = tema::botao_principal(ui, "Adicionar agente", pode).clicked();
                        if tem_claude {
                            ui.add_space(8.0);
                            retomar = tema::botao_secundario_com(ui, "Retomar conversa do Claude Code", pode).clicked();
                        }
                    });
                });
            });
        });
        if novo || retomar {
            self.adicionar_agente(id, retomar);
        }
    }

    /// A aba Arquivos: árvore e busca por nome, o arquivo ou o diff, a busca por texto.
    fn aba_arquivos(&mut self, ui: &mut egui::Ui, id: i64, pasta: &str, agora: f64) {
        let ctx = ui.ctx().clone();
        let area = ui.available_rect_before_wrap();
        self.ancora_aviso = Some(area.center_bottom() - vec2(0.0, 16.0));
        let editor = self.editor;
        let aba = self.abas_arquivos.entry(id).or_insert_with(|| arquivos::AbaArquivos::nova(&ctx, id));
        let pedido = aba.mostrar(ui, area, pasta, editor.map(|(nome, _)| nome));
        ui.allocate_rect(area, Sense::hover());
        match pedido {
            None => {}
            Some(gaveta::PedidoGaveta::AbrirPasta) => {
                if let Err(e) = sistema::abrir_pasta(pasta) {
                    self.erro(format!("Não consegui abrir a pasta: {e}"), agora);
                }
            }
            Some(gaveta::PedidoGaveta::AbrirNoEditor(arquivo, linha)) => {
                if let Some((nome, comando)) = editor
                    && let Err(e) = sistema::abrir_na_linha(comando, &arquivo, linha)
                {
                    self.erro(format!("Não consegui abrir no {nome}: {e}"), agora);
                }
            }
            Some(gaveta::PedidoGaveta::AbrirNoSistema(arquivo)) => {
                if let Err(e) = sistema::abrir_no_sistema(&arquivo) {
                    self.erro(format!("Não consegui abrir: {e}"), agora);
                }
            }
            // A citação vai para a caixa de mensagem, no Terminal.
            Some(gaveta::PedidoGaveta::Citar(texto)) => {
                self.compositor.citar(&texto);
                self.mudar_aba(id, Aba::Terminal, &ctx);
            }
        }
    }

    /// A aba Navegador e Play: o navegador da tarefa (abrir, ir para um
    /// endereço, capturar, fechar) e o Play do projeto na pasta da tarefa,
    /// com a saída embaixo.
    fn aba_navegador(&mut self, ui: &mut egui::Ui, id: i64, projeto: i64, agora: f64) {
        let p = cores();
        let pode = self.pode_mudar();
        let aberto = self.modelo.navegadores.contains(&id);
        let area = ui.available_rect_before_wrap();
        self.ancora_aviso = Some(area.center_bottom() - vec2(0.0, 16.0));
        let mut acao: Option<(&str, Rect)> = None;
        // O navegador: uma faixa com o estado e os botões.
        let faixa = Rect::from_min_size(area.min, vec2(area.width(), 92.0));
        ui.painter().rect(faixa, CornerRadius::same(14), p.superficie, Stroke::new(1.0, p.borda.gamma_multiply(0.7)), StrokeKind::Inside);
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(faixa.shrink2(vec2(16.0, 12.0))).layout(egui::Layout::top_down(egui::Align::Min)));
        filho.horizontal(|ui| {
            tema::desenhar_icone(ui.painter(), ui.cursor().min + vec2(8.0, 11.0), tema::Icone::Globo, p.texto);
            ui.add_space(22.0);
            ui.label(texto_forte("Navegador da tarefa", 14.5).color(p.texto));
            let (texto, cor) = if aberto { ("aberto ao lado da Colmeia", p.ok) } else { ("fechado", p.suave) };
            ui.label(RichText::new(texto).size(12.5).color(cor));
            if let Some(url) = self.enderecos.get(&id) {
                ui.label(RichText::new(url).size(12.0).color(p.suave).monospace());
            }
        });
        filho.add_space(8.0);
        filho.horizontal(|ui| {
            let rotulo = if aberto { "Trazer para frente" } else { "Abrir navegador" };
            let r = tema::botao_principal(ui, rotulo, pode);
            if r.clicked() {
                acao = Some(("abrir", r.rect));
            }
            ui.add_space(6.0);
            let r = tema::botao_secundario_com(ui, "Ir para endereço…", pode);
            if r.clicked() {
                acao = Some(("endereco", r.rect));
            }
            ui.add_space(6.0);
            let r = tema::botao_secundario_com(ui, "Capturar", pode && aberto).on_hover_text("Anexa uma captura do navegador à tarefa (Ctrl+Shift+B)");
            if r.clicked() {
                acao = Some(("capturar", r.rect));
            }
            ui.add_space(6.0);
            let r = tema::botao_secundario_com(ui, "Fechar", pode && aberto);
            if r.clicked() {
                acao = Some(("fechar", r.rect));
            }
        });
        // O Play: o botão do projeto e a saída da execução.
        let resto = Rect::from_min_max(pos2(area.left(), faixa.bottom() + 12.0), area.max);
        ui.painter().rect(resto, CornerRadius::same(14), p.superficie, Stroke::new(1.0, p.borda.gamma_multiply(0.7)), StrokeKind::Inside);
        let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(resto.shrink2(vec2(16.0, 12.0))).layout(egui::Layout::top_down(egui::Align::Min)));
        let nome_projeto = self.modelo.projetos.iter().find(|p| p.id == projeto).map(|p| p.nome.clone()).unwrap_or_default();
        let bytes = self.bytes.clone();
        let mut aviso = None;
        filho.horizontal(|ui| {
            tema::play(ui.painter(), ui.cursor().min + vec2(8.0, 16.0), 5.0, if self.play.rodando(projeto) { p.ok } else { p.suave });
            ui.add_space(22.0);
            ui.label(texto_forte("Rodar", 14.5).color(p.texto));
            ui.label(RichText::new("na pasta da tarefa · Shift+F10 roda, Ctrl+F2 para").size(12.5).color(p.suave));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                aviso = self.play.botao(ui, &bytes, projeto, &nome_projeto, id, pode);
            });
        });
        filho.add_space(10.0);
        if self.play.painel_do_projeto(projeto) && !self.demo {
            if let Some(a) = self.play.mostrar_painel(&mut filho, &bytes, pode) {
                aviso = Some(a);
            }
        } else {
            filho.label(RichText::new("Nada rodando agora. A saída aparece aqui, sem cobrir o terminal dos agentes.").size(13.0).color(p.suave));
        }
        if let Some((tipo, texto)) = aviso {
            self.avisar(tipo, &texto, agora);
        }
        let ctx = ui.ctx().clone();
        match acao {
            Some(("abrir", botao)) => self.clique_navegador(id, botao, &ctx, agora),
            Some(("endereco", botao)) => self.pedir_endereco(id, botao),
            Some(("capturar", _)) => self.capturar_navegador(id, &ctx),
            Some(("fechar", _)) => {
                let envio = self.respostas_navegador.0.clone();
                std::thread::spawn(move || {
                    let _ = envio.send(RespostaNavegador::Fechado(api::fechar_navegador(id)));
                    ctx.request_repaint();
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn fichas_dividem_a_fileira_com_limite() {
        assert_eq!(larguras_das_fichas(1000.0, 0), 0.0);
        assert_eq!(larguras_das_fichas(1000.0, 1), 280.0);
        assert_eq!(larguras_das_fichas(608.0, 3), 200.0);
        assert_eq!(larguras_das_fichas(300.0, 6), 120.0);
    }

    #[test]
    fn abas_na_ordem_dos_atalhos() {
        assert_eq!(Aba::TODAS.iter().map(|a| a.indice()).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(Aba::default(), Aba::Terminal);
    }
}
