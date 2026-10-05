//! Pedir ao agente: a caixa que abre no cartão da Daily e da Sprint e no
//! slide da apresentação, e o estado de um pedido como a tela mostra (a
//! pílula, a faixa e a linha acima da nota).
//!
//! O núcleo escolhe o agente, põe o pedido na fila e entrega quando o agente
//! espera você; a resposta volta para a nota e para os anexos da tarefa.
//! Nada aqui roda por tempo: a caixa só redesenha com entrada ou resposta.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke, vec2};

use crate::api;
use crate::dados::{AgenteTela, EstadoAgente, Modelo, PEDE_APROVACAO};
use crate::registro;
use crate::tema::{self, EstadoVisual, Pilula, cores, forte};

/// Maior pedido, em caracteres (o núcleo confere de novo).
pub const MAX_PEDIDO: usize = 2000;
/// A partir daqui o contador aparece.
const AVISO_TAMANHO: usize = 1800;
/// Largura da caixa.
pub const LARGURA: f32 = 460.0;

/// Exemplos que preenchem o campo (não enviam).
pub const EXEMPLOS: [&str; 2] = ["Traga os números (ex.: total de testes) e complemente a nota", "Capture prints das telas finalizadas e anexe à nota"];

/// O que a tela oferece para um pedido, conforme o estado dele.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AcaoPedido {
    Cancelar,
    AbrirTarefa,
    PedirDeNovo,
}

/// Como um pedido aparece: a pílula curta, o texto longo da faixa, a cor e a ação.
#[derive(Clone, Debug, PartialEq)]
pub struct Estado {
    pub curto: String,
    pub longo: String,
    pub cor: Color32,
    pub cheio: bool,
    pub acao: Option<AcaoPedido>,
    /// Ainda espera o agente (fila ou entregue).
    pub aberto: bool,
    /// O agente espera você no terminal (aprovar ou responder).
    pub pede_voce: bool,
}

/// O pedido que a tarefa mostra e o estado dele.
#[derive(Clone, Debug)]
pub struct Resumo {
    pub pedido: api::Pedido,
    pub estado: Estado,
}

pub type Pedidos = HashMap<i64, Resumo>;

/// Os primeiros 19 caracteres de um momento RFC 3339 ("2026-10-02T21:40:05"):
/// dá para comparar dois momentos do núcleo sem lidar com fuso.
fn ate_o_segundo(momento: &str) -> &str {
    momento.get(..19).unwrap_or(momento)
}

/// O estado do pedido, juntando o do agente que vai responder.
pub fn estado(p: &api::Pedido, agente: Option<&AgenteTela>) -> Estado {
    let c = cores();
    let ativo = agente.is_some_and(|a| a.ativo);
    let aguardando = agente.is_some_and(|a| a.ativo && a.estado == EstadoAgente::Aguardando);
    let aprovacao = aguardando && agente.is_some_and(|a| a.motivo == PEDE_APROVACAO);
    let novo = |curto: &str, longo: String, cor: Color32, cheio: bool, acao: Option<AcaoPedido>| Estado {
        curto: curto.into(),
        longo,
        cor,
        cheio,
        acao,
        aberto: p.aberto(),
        pede_voce: aprovacao,
    };
    match p.estado.as_str() {
        "fila" if aprovacao => novo("Aprovar no terminal", "Esperando você aprovar no terminal".into(), c.alerta, true, Some(AcaoPedido::AbrirTarefa)),
        "fila" if !ativo => novo("Abrindo o Claude Code…", "Abrindo o Claude Code…".into(), c.suave, false, Some(AcaoPedido::Cancelar)),
        "fila" => novo("Na fila", "Na fila · entra quando o agente parar".into(), c.suave, false, Some(AcaoPedido::Cancelar)),
        "entregue" if aprovacao => novo("Aprovar no terminal", "Esperando você aprovar no terminal".into(), c.alerta, true, Some(AcaoPedido::AbrirTarefa)),
        // Voltou a esperar depois da entrega sem responder pela Colmeia: pode
        // ter feito uma pergunta no terminal.
        "entregue" if aguardando && agente.is_some_and(|a| ate_o_segundo(&a.desde_em) > ate_o_segundo(&p.entregue_em)) => {
            let mut e = novo("Parou sem responder", "O agente parou sem responder aqui".into(), c.alerta, false, Some(AcaoPedido::AbrirTarefa));
            e.pede_voce = true;
            e
        }
        "entregue" => {
            novo(&format!("Com o agente · {}", p.entregue_hora), format!("Com o agente desde {}", p.entregue_hora), c.ok, true, Some(AcaoPedido::AbrirTarefa))
        }
        "respondido" => novo(&format!("Respondido · {}", p.respondido_hora), format!("Respondido às {}", p.respondido_hora), c.destaque, true, None),
        "falhou" => {
            let motivo = if p.motivo.is_empty() { String::new() } else { format!(": {}", p.motivo) };
            novo("Não deu certo", format!("Não deu certo{motivo}"), c.erro, true, Some(AcaoPedido::PedirDeNovo))
        }
        _ => novo("Cancelado", "Pedido cancelado".into(), c.suave, false, None),
    }
}

/// O pedido de cada tarefa (o aberto ou o último fechado), com o estado.
pub fn resumir(modelo: &Modelo) -> Pedidos {
    let mut resumo = Pedidos::new();
    for t in &modelo.tarefas {
        if let Some(p) = modelo.pedido_da_tarefa(t.id) {
            let agente = t.agentes.iter().find(|a| a.id == p.agente_id);
            resumo.insert(t.id, Resumo { pedido: p.clone(), estado: estado(p, agente) });
        }
    }
    resumo
}

/// A pílula de um estado (22 de altura).
pub fn pilula(e: &Estado) -> Pilula<'_> {
    Pilula { texto: &e.curto, cor: e.cor, cheio: e.cheio, ponto: true, grande: false }
}

/// O que a caixa pede a quem a mostra.
pub enum Saida {
    Fechar,
    Enviado(Box<api::Pedido>),
    AbrirTarefa,
}

enum Resposta {
    Destino(Result<api::Destino, String>),
    Enviado(Result<api::Pedido, String>),
    Cancelado(Result<(), String>),
}

/// A caixa "Pedir ao agente" de uma tarefa. O rascunho fica em `texto`, que
/// quem mostra guarda por tarefa quando a caixa fecha.
pub struct Caixa {
    pub tarefa: i64,
    pub texto: String,
    tipo: String,
    periodo: String,
    destino: Option<Result<api::Destino, String>>,
    erro: Option<String>,
    enviando: bool,
    focar: bool,
    /// Quadros desde que abriu: o clique que abriu não fecha.
    quadros: u32,
    /// Maior distância já vista do topo da caixa ao rodapé: a caixa não
    /// encolhe quando os exemplos somem, e o Enviar fica onde estava.
    topo_rodape: f32,
    canal: (Sender<Resposta>, Receiver<Resposta>),
    /// Composição de acento (IME) em andamento: o Enter dela não envia.
    composicao: crate::teclas::Composicao,
}

impl Caixa {
    /// Abre a caixa e já pergunta ao núcleo para quem o pedido vai.
    pub fn nova(ctx: &egui::Context, tarefa: i64, tipo: &str, periodo: &str, texto: String) -> Caixa {
        let caixa = Caixa {
            tarefa,
            texto,
            tipo: tipo.into(),
            periodo: periodo.into(),
            destino: None,
            erro: None,
            enviando: false,
            focar: true,
            quadros: 0,
            topo_rodape: 0.0,
            canal: mpsc::channel(),
            composicao: Default::default(),
        };
        registro::em_segundo_plano(&caixa.canal.0, ctx, move || Resposta::Destino(api::destino_do_pedido(tarefa)));
        caixa
    }

    fn bloqueado(&self) -> bool {
        matches!(&self.destino, Some(Ok(d)) if d.acao == "bloqueado")
    }

    fn receber(&mut self) -> Option<Saida> {
        let mut saida = None;
        while let Ok(r) = self.canal.1.try_recv() {
            match r {
                Resposta::Destino(d) => self.destino = Some(d),
                Resposta::Enviado(Ok(p)) => {
                    self.enviando = false;
                    saida = Some(Saida::Enviado(Box::new(p)));
                }
                Resposta::Enviado(Err(e)) => {
                    self.enviando = false;
                    self.erro = Some(e);
                }
                Resposta::Cancelado(Err(e)) => self.erro = Some(format!("Não cancelei: {e}")),
                Resposta::Cancelado(Ok(())) => {}
            }
        }
        saida
    }

    fn enviar(&mut self, ctx: &egui::Context) {
        let (tarefa, texto, tipo, periodo) = (self.tarefa, self.texto.trim().to_string(), self.tipo.clone(), self.periodo.clone());
        self.enviando = true;
        self.erro = None;
        registro::em_segundo_plano(&self.canal.0, ctx, move || Resposta::Enviado(api::criar_pedido(tarefa, &texto, &tipo, &periodo)));
    }

    /// Desenha a caixa presa a `ancora` (com `pivo`). `atual`: o pedido que a
    /// tarefa já tem. Devolve o que a caixa pede.
    pub fn mostrar(&mut self, ctx: &egui::Context, ancora: Pos2, pivo: Align2, atual: Option<&Resumo>) -> Option<Saida> {
        let mut saida = self.receber();
        if saida.is_some() {
            return saida;
        }
        let p = cores();
        let n = self.texto.chars().count();
        let pode_enviar = !self.texto.trim().is_empty() && n <= MAX_PEDIDO && !self.enviando && !self.bloqueado() && self.destino.is_some();
        // Esc fecha (o rascunho fica). Enter envia e Shift+Enter quebra a
        // linha, só com o campo em foco: com o foco num botão de exemplo, o
        // Enter é do botão (põe o texto e devolve o foco ao campo).
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            return Some(Saida::Fechar);
        }
        let id = Id::new(("campo-pedido", self.tarefa));
        let com_foco = ctx.memory(|m| m.has_focus(id));
        let enter = com_foco && ctx.input_mut(|i| self.composicao.tirar_enters(&mut i.events));
        let mut enviar = enter && pode_enviar;
        let mut cancelar = None;
        let area = egui::Area::new(Id::new("caixa-pedido")).order(egui::Order::Foreground).pivot(pivo).fixed_pos(ancora).show(ctx, |ui| {
            tema::moldura_flutuante().show(ui, |ui| {
                ui.set_width(LARGURA - 32.0);
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.horizontal(|ui| {
                    ui.label(tema::texto_forte("Pedir ao agente", 15.0).color(p.texto));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tema::botao_icone(ui, tema::Icone::Fechar, 24.0).on_hover_text("Fechar (Esc)").clicked() {
                            saida = Some(Saida::Fechar);
                        }
                    });
                });
                self.linha_destino(ui);
                if let Some(r) = atual.filter(|r| r.pedido.estado != "cancelado") {
                    match self.bloco_atual(ui, r) {
                        Some(AcaoPedido::Cancelar) => cancelar = Some(r.pedido.id),
                        Some(AcaoPedido::AbrirTarefa) => saida = Some(Saida::AbrirTarefa),
                        Some(AcaoPedido::PedirDeNovo) => {
                            self.texto = r.pedido.texto.clone();
                            self.focar = true;
                        }
                        None => {}
                    }
                }
                let resposta = tema::campo_mensagem(ui, &mut self.texto, 3, 120.0, id, self.enviando);
                // Foco no campo uma vez só (pedir em todo quadro trava os eventos).
                if std::mem::take(&mut self.focar) {
                    resposta.request_focus();
                }
                if self.texto.is_empty() {
                    ui.label(RichText::new("Exemplos").color(p.suave).size(12.5));
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for exemplo in EXEMPLOS {
                        if tema::botao_secundario(ui, exemplo).clicked() {
                            self.texto = exemplo.into();
                            self.focar = true;
                        }
                    }
                    ui.spacing_mut().item_spacing.y = 10.0;
                }
                if let Some(e) = &self.erro {
                    ui.label(RichText::new(e).color(p.erro).size(12.5));
                }
                ui.add_space(2.0);
                let topo = ui.cursor().top() - ui.min_rect().top();
                if topo < self.topo_rodape {
                    ui.add_space(self.topo_rodape - topo);
                }
                self.topo_rodape = self.topo_rodape.max(topo);
                ui.horizontal(|ui| {
                    ui.set_height(34.0);
                    ui.label(RichText::new("Enter envia · Shift+Enter nova linha · Esc fecha").color(p.suave).size(12.0));
                    if n >= AVISO_TAMANHO {
                        let cor = if n >= MAX_PEDIDO { p.erro } else { p.alerta };
                        ui.label(RichText::new(format!("{} / {}", milhar(n), milhar(MAX_PEDIDO))).color(cor).size(12.5));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let rotulo = if self.enviando { "Enviando…" } else { "Enviar" };
                        enviar |= tema::botao_principal(ui, rotulo, pode_enviar).clicked();
                    });
                });
            });
        });
        // Clique fora fecha (o rascunho fica guardado por quem mostra).
        let fora = ctx.input(|i| i.pointer.any_released() && i.pointer.interact_pos().is_some_and(|pos| !area.response.rect.contains(pos)));
        self.quadros = self.quadros.saturating_add(1);
        if fora && self.quadros > 1 && saida.is_none() {
            saida = Some(Saida::Fechar);
        }
        if let Some(id) = cancelar {
            registro::em_segundo_plano(&self.canal.0, ctx, move || Resposta::Cancelado(api::cancelar_pedido(id)));
        }
        if enviar && saida.is_none() {
            self.enviar(ctx);
        }
        saida
    }

    /// Para quem vai: o agente ativo, o que vai iniciar de novo, um novo ou o
    /// motivo do bloqueio (antes de escrever, não depois).
    fn linha_destino(&self, ui: &mut egui::Ui) {
        let p = cores();
        let destino = match &self.destino {
            None => {
                let (r, _) = ui.allocate_exact_size(vec2(220.0, 20.0), Sense::hover());
                tema::esqueleto(ui.painter(), Rect::from_min_size(r.min + vec2(0.0, 3.0), vec2(220.0, 14.0)), tema::RAIO_ETIQUETA);
                return;
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("Não sei para quem vai: {e}")).color(p.erro).size(12.5));
                return;
            }
            Some(Ok(d)) => d,
        };
        if destino.acao == "bloqueado" {
            egui::Frame::new()
                .fill(tema::fundo_tingido(p, p.alerta, tema::claro()))
                .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, p.alerta);
                        ui.add(egui::Label::new(RichText::new(&destino.motivo).color(p.texto).size(13.0)).wrap());
                    });
                });
            return;
        }
        let nome = destino.agente.as_ref().map(|a| format!("Claude Code · {}", a.papel)).unwrap_or_else(|| "Claude Code".into());
        let (visual, antes, forte_txt, depois) = match destino.acao.as_str() {
            "ativo" => {
                let aguardando = destino.agente.as_ref().is_some_and(|a| a.estado == "aguardando");
                let visual = if aguardando { EstadoVisual::SuaVez } else { EstadoVisual::Trabalhando };
                let depois = if aguardando { " (esperando você)" } else { " (trabalhando: entra quando ele parar)" };
                (visual, "Vai para ", nome, depois.to_string())
            }
            "reiniciar" => (EstadoVisual::Parado, "Vai iniciar de novo ", nome, String::new()),
            _ if destino.outros => (EstadoVisual::Parado, "Os agentes desta tarefa não são Claude Code; vou abrir um", String::new(), String::new()),
            _ if destino.conversa.is_empty() => {
                (EstadoVisual::Parado, "Não há agente: vou abrir o Claude Code numa conversa nova", String::new(), String::new())
            }
            _ => (EstadoVisual::Parado, "Não há agente: vou abrir o Claude Code retomando ", String::new(), format!("«{}»", destino.conversa)),
        };
        ui.horizontal(|ui| {
            ui.set_height(20.0);
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 20.0), Sense::hover());
            tema::ponto(ui.painter(), r.center(), 4.0, visual);
            let largura = ui.available_width();
            let mut job = egui::text::LayoutJob::default();
            let normal = egui::TextFormat::simple(FontId::proportional(13.0), p.texto);
            job.append(antes, 0.0, normal.clone());
            if !forte_txt.is_empty() {
                job.append(&forte_txt, 0.0, egui::TextFormat::simple(forte(13.0), p.texto));
            }
            job.append(&depois, 0.0, normal);
            job.wrap = egui::text::TextWrapping { max_width: largura, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
            let galeria = ui.painter().layout_job(job);
            let cortou = galeria.elided;
            let resposta = ui.label(galeria);
            if cortou {
                resposta.on_hover_text(format!("{antes}{forte_txt}{depois}"));
            }
        });
    }

    /// O pedido que a tarefa já tem: a pílula, o texto e a ação.
    fn bloco_atual(&self, ui: &mut egui::Ui, r: &Resumo) -> Option<AcaoPedido> {
        let p = cores();
        let mut acao = None;
        egui::Frame::new().fill(p.superficie).corner_radius(CornerRadius::same(tema::RAIO_CONTROLE)).inner_margin(egui::Margin::symmetric(10, 8)).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(36.0 - 16.0);
                ui.horizontal(|ui| {
                    let pilula = pilula(&r.estado);
                    let largura = pilula.largura(ui.painter());
                    let (rect, _) = ui.allocate_exact_size(vec2(largura, 22.0), Sense::hover());
                    pilula.pintar(ui.painter(), rect.min);
                    let rotulo = match r.estado.acao {
                        Some(AcaoPedido::Cancelar) => Some("Cancelar"),
                        Some(AcaoPedido::AbrirTarefa) => Some("Abrir tarefa"),
                        Some(AcaoPedido::PedirDeNovo) => Some("Pedir de novo"),
                        None => None,
                    };
                    let largura_botao =
                        rotulo.map_or(0.0, |t| ui.painter().layout_no_wrap(t.into(), FontId::proportional(13.0), p.texto).size().x + 28.0 + 8.0);
                    let largura_texto = (ui.available_width() - largura_botao).max(40.0);
                    let formato = egui::TextFormat { font_id: FontId::proportional(13.0), color: p.texto, italics: true, ..Default::default() };
                    let galeria = tema::cortar(ui.painter(), &r.pedido.texto.replace('\n', " "), formato, largura_texto, 1, true);
                    // O texto vem logo depois da pílula (a sobra fica no fim da
                    // linha, antes da ação), seja qual for o tamanho do pedido.
                    let (area, resposta) = ui.allocate_exact_size(vec2(largura_texto, 22.0), Sense::hover());
                    let topo = area.center().y - galeria.size().y / 2.0;
                    ui.painter().galley(Pos2::new(area.left(), topo), galeria, p.texto);
                    resposta.on_hover_text(&r.pedido.texto);
                    if let (Some(t), Some(a)) = (rotulo, r.estado.acao)
                        && tema::botao_secundario(ui, t).clicked()
                    {
                        acao = Some(a);
                    }
                });
                if r.pedido.estado == "falhou" || r.estado.pede_voce {
                    let cor = if r.pedido.estado == "falhou" { p.erro } else { p.texto };
                    ui.label(RichText::new(&r.estado.longo).color(cor).size(13.0));
                }
            },
        );
        acao
    }
}

/// 1800 vira "1.800".
fn milhar(n: usize) -> String {
    let s = n.to_string();
    let mut saida = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            saida.push('.');
        }
        saida.push(c);
    }
    saida
}

/// O botão "Pedir ao agente" num retângulo fixo (no canto do cartão):
/// secundário, ou no estado ativo do `chip_alternar` com a caixa aberta.
pub fn botao_no_cartao(ui: &mut egui::Ui, rect: Rect, id: Id, texto: &str, aberta: bool, ativo: bool) -> egui::Response {
    let p = cores();
    let resposta = ui.interact(rect, id, if ativo { Sense::click() } else { Sense::hover() });
    let (fundo, borda, cor) = match (ativo, aberta, resposta.hovered()) {
        (false, _, _) => (Color32::TRANSPARENT, p.borda.gamma_multiply(0.5), p.suave),
        (true, true, _) => (p.destaque.gamma_multiply(if tema::claro() { 0.12 } else { 0.18 }), p.destaque.gamma_multiply(0.6), p.texto),
        (true, false, true) => (p.realce, p.borda, p.texto),
        (true, false, false) => (p.superficie_alta, p.borda, p.texto),
    };
    let pintor = ui.painter();
    pintor.rect(rect, CornerRadius::same(16), fundo, Stroke::new(1.0, borda), egui::StrokeKind::Inside);
    let fonte = if aberta { forte(13.0) } else { FontId::proportional(13.0) };
    pintor.text(rect.center(), Align2::CENTER_CENTER, texto, fonte, cor);
    if ativo { resposta.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resposta }
}

/// Largura do botão do cartão para o texto.
pub fn largura_botao(pintor: &egui::Painter, texto: &str) -> f32 {
    pintor.layout_no_wrap(texto.to_owned(), FontId::proportional(13.0), Color32::WHITE).size().x + 28.0
}

/// Texto do botão no cartão: "Pedir" em cartões estreitos.
pub fn texto_botao(largura_cartao: f32) -> &'static str {
    if largura_cartao < 340.0 { "Pedir" } else { "Pedir ao agente" }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pedido(estado: &str) -> api::Pedido {
        api::Pedido {
            id: 1,
            tarefa_id: 2,
            agente_id: 3,
            tipo: "daily".into(),
            periodo: "2026-10-02".into(),
            texto: "Traga o total de testes".into(),
            estado: estado.into(),
            entregue_em: "2026-10-02T21:40:05.123Z".into(),
            entregue_hora: "18:40".into(),
            respondido_hora: "18:44".into(),
            ..Default::default()
        }
    }

    fn agente(estado: EstadoAgente, motivo: &str, desde: &str) -> AgenteTela {
        AgenteTela {
            id: 3,
            ferramenta: "claude".into(),
            papel: "dev".into(),
            ativo: true,
            estado,
            motivo: motivo.into(),
            desde: String::new(),
            desde_em: desde.into(),
            fim: None,
            erro_visto: true,
        }
    }

    #[test]
    fn estados_do_pedido_como_a_tela_mostra() {
        let trabalhando = agente(EstadoAgente::Trabalhando, "", "2026-10-02T21:39:00Z");
        let aprovacao = agente(EstadoAgente::Aguardando, PEDE_APROVACAO, "2026-10-02T21:39:00Z");
        assert_eq!(estado(&pedido("fila"), Some(&trabalhando)).curto, "Na fila");
        assert_eq!(estado(&pedido("fila"), Some(&trabalhando)).acao, Some(AcaoPedido::Cancelar));
        assert_eq!(estado(&pedido("fila"), Some(&aprovacao)).curto, "Aprovar no terminal");
        assert_eq!(estado(&pedido("fila"), None).curto, "Abrindo o Claude Code…");
        assert_eq!(estado(&pedido("entregue"), Some(&trabalhando)).curto, "Com o agente · 18:40");
        // Esperando desde antes da entrega: ainda não começou. Depois: parou sem responder.
        let antes = agente(EstadoAgente::Aguardando, "esperando resposta", "2026-10-02T21:40:05Z");
        assert_eq!(estado(&pedido("entregue"), Some(&antes)).curto, "Com o agente · 18:40");
        let depois = agente(EstadoAgente::Aguardando, "esperando resposta", "2026-10-02T21:43:00Z");
        assert_eq!(estado(&pedido("entregue"), Some(&depois)).curto, "Parou sem responder");
        assert_eq!(estado(&pedido("respondido"), None).longo, "Respondido às 18:44");
        let mut falhou = pedido("falhou");
        falhou.motivo = "o agente foi encerrado".into();
        let e = estado(&falhou, None);
        assert_eq!((e.longo.as_str(), e.acao), ("Não deu certo: o agente foi encerrado", Some(AcaoPedido::PedirDeNovo)));
    }

    #[test]
    fn numeros_com_ponto_de_milhar_e_botao_curto() {
        assert_eq!(milhar(1850), "1.850");
        assert_eq!(milhar(2000), "2.000");
        assert_eq!(milhar(950), "950");
        assert_eq!(texto_botao(320.0), "Pedir");
        assert_eq!(texto_botao(520.0), "Pedir ao agente");
    }
}
