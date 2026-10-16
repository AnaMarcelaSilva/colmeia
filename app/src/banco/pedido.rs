//! O pedido de consulta de um agente aberto por inteiro, numa gaveta por
//! cima do console: quem pede e em que tarefa, a conexão, o banco e o prazo,
//! a instrução travada, "Rodar para mim antes" (roda no seu console e o
//! resultado fica só com você), o que esse agente já pediu nesta conexão e,
//! no pé, o motivo, Recusar e Aprovar e enviar. A resposta é a mesma do
//! cartão do painel da tarefa (`aprovacao::Resposta`), com a senha quando falta.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};

use super::aprovacao::{Estado, Resposta};
use super::{conexao, console, sql};
use crate::api;
use crate::tema::{self, Icone, cores, forte};

const LARGURA: f32 = 600.0;
const LINHA_TABELA: f32 = 26.0;

/// O que a gaveta mostra além do pedido.
pub struct Vista<'a> {
    pub a: &'a api::Aprovacao,
    pub branch: &'a str,
    pub chaveiro: bool,
    pub rodada: Option<Rodada<'a>>,
    /// A instrução está rodando no seu console agora.
    pub rodando: bool,
    /// O que esse agente pediu antes nesta conexão (`None`: carregando).
    pub antes: Option<Vec<&'a api::ConsultaFeita>>,
}

/// O que deu "Rodar para mim antes".
pub enum Rodada<'a> {
    Resultado { r: &'a api::Resultado, hora: &'a str },
    Erro(&'a str),
}

pub enum Pedido {
    Fechar,
    AbrirTarefa,
    Rodar,
    Responder(Resposta),
}

/// A gaveta, por cima do cartão do console. Clicar no véu ou Esc fecha.
pub fn gaveta(ctx: &egui::Context, cartao: Rect, v: &Vista, estado: &mut Estado) -> Option<Pedido> {
    let p = cores();
    let mut pedido = None;
    egui::Area::new(Id::new("gaveta-pedido")).order(egui::Order::Middle).fixed_pos(cartao.min).show(ctx, |ui| {
        let (veu, resposta) = ui.allocate_exact_size(cartao.size(), Sense::click());
        ui.painter().rect_filled(veu, CornerRadius::same(tema::RAIO_SUPERFICIE), p.janela.gamma_multiply(0.55));
        if resposta.clicked() {
            pedido = Some(Pedido::Fechar);
        }
        let largura = LARGURA.min(veu.width() - 20.0);
        let rect = Rect::from_min_max(pos2(veu.right() - 10.0 - largura, veu.top() + 10.0), veu.right_bottom() - vec2(10.0, 10.0));
        // O clique no fundo da gaveta não chega ao véu.
        ui.interact(rect, Id::new("fundo-gaveta-pedido"), Sense::click());
        tema::cartao_solto(ui.painter(), rect, p.superficie_alta);
        ui.painter().rect_stroke(rect, CornerRadius::same(tema::RAIO_MOLDURA), Stroke::new(1.0, p.alerta.gamma_multiply(0.55)), StrokeKind::Inside);
        let alto_pe = altura_do_pe(v, estado);
        let topo = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + 132.0));
        ui.painter().rect_filled(
            topo.shrink(1.0),
            CornerRadius { nw: tema::RAIO_MOLDURA, ne: tema::RAIO_MOLDURA, sw: 0, se: 0 },
            tema::fundo_tingido(p, p.alerta, tema::claro()),
        );
        let mut cima = ui.new_child(UiBuilder::new().max_rect(topo.shrink2(vec2(18.0, 14.0))));
        if let Some(x) = cabecalho(&mut cima, v) {
            pedido = Some(x);
        }
        let pe = Rect::from_min_max(pos2(rect.left(), rect.bottom() - alto_pe), rect.max);
        ui.painter().rect_filled(
            pe.shrink(1.0),
            CornerRadius { nw: 0, ne: 0, sw: tema::RAIO_MOLDURA, se: tema::RAIO_MOLDURA },
            tema::fundo_tingido(p, p.alerta, tema::claro()),
        );
        ui.painter().hline(pe.x_range().shrink(1.0), pe.top(), Stroke::new(1.0, p.alerta.gamma_multiply(0.4)));
        let meio = Rect::from_min_max(pos2(rect.left() + 18.0, topo.bottom() + 6.0), pos2(rect.right() - 18.0, pe.top() - 8.0));
        let mut miolo = ui.new_child(UiBuilder::new().max_rect(meio));
        egui::ScrollArea::vertical().id_salt("miolo-pedido").auto_shrink(false).show(&mut miolo, |ui| {
            if let Some(x) = corpo(ui, v) {
                pedido = Some(x);
            }
        });
        let mut baixo = ui.new_child(UiBuilder::new().max_rect(pe.shrink2(vec2(14.0, 12.0))));
        if let Some(x) = rodape(&mut baixo, v, estado) {
            pedido = Some(x);
        }
    });
    if pedido.is_none() && !estado.enviando && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        pedido = Some(Pedido::Fechar);
    }
    pedido
}

fn altura_do_pe(v: &Vista, estado: &Estado) -> f32 {
    let mut alto = 32.0 + 24.0 + 22.0;
    if v.a.precisa_senha {
        alto += 64.0 + if v.chaveiro { 26.0 } else { 0.0 } + if estado.erro_senha.is_some() { 20.0 } else { 0.0 };
    }
    if estado.erro.is_some() {
        alto += 22.0;
    }
    alto
}

fn cabecalho(ui: &mut egui::Ui, v: &Vista) -> Option<Pedido> {
    let p = cores();
    let a = v.a;
    let mut pedido = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 24.0), Sense::hover());
        ui.painter().circle_filled(r.left_center() + vec2(4.0, 0.0), 4.5, p.alerta);
        let quem = if a.agente.is_empty() { "O agente" } else { a.agente.as_str() };
        ui.label(tema::texto_forte(quem, 16.0).color(p.texto));
        ui.label(RichText::new(" quer consultar ").color(p.suave).size(16.0));
        ui.label(tema::texto_forte(&a.conexao, 16.0).color(p.texto));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tema::botao_icone(ui, Icone::Fechar, 28.0).on_hover_text("Fechar (Esc)").clicked() {
                pedido = Some(Pedido::Fechar);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        if !a.tarefa.is_empty() {
            ui.label(RichText::new("na tarefa").color(p.suave).size(12.5));
            let g = tema::cortar(ui.painter(), &a.tarefa, egui::TextFormat::simple(FontId::proportional(12.5), p.texto), 260.0, 1, true);
            let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
            ui.painter().galley(r.min, g, p.texto);
        }
        if !v.branch.is_empty() {
            tema::etiqueta_ui(ui, v.branch, FontId::monospace(11.0), p.suave);
        }
        if tema::link_em_linha(ui, "Abrir na tarefa").clicked() {
            pedido = Some(Pedido::AbrirTarefa);
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        ui.spacing_mut().item_spacing.x = 26.0;
        let limite = if a.limite > 0 { a.limite } else { 200 };
        let banco = if a.banco.is_empty() { "padrão".to_string() } else { a.banco.clone() };
        let prazo = if a.expira_hora.is_empty() { "5 min".to_string() } else { format!("até {}", a.expira_hora) };
        let fichas = [
            ("CONEXÃO", format!("{} · {}", console::sigla_do_tipo(&a.tipo), a.conexao), p.texto),
            ("BANCO", banco, p.texto),
            ("MODO", "Só leitura".to_string(), p.texto),
            ("DEVOLVE", format!("até {limite} linhas"), p.texto),
            ("PRAZO", prazo, p.alerta),
        ];
        for (rotulo, valor, cor) in fichas {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.label(tema::texto_forte(rotulo, 10.5).color(p.suave));
                let g = tema::cortar(ui.painter(), &valor, egui::TextFormat::simple(forte(13.0), cor), 150.0, 1, true);
                let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                ui.painter().galley(r.min, g, cor);
            });
        }
    });
    pedido
}

fn corpo(ui: &mut egui::Ui, v: &Vista) -> Option<Pedido> {
    let p = cores();
    let a = v.a;
    let mut pedido = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(tema::texto_forte("A instrução", 13.0).color(p.texto));
        ui.label(RichText::new("travada · roda exatamente assim").color(p.suave).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tema::botao_secundario(ui, "Copiar").clicked() {
                ui.ctx().copy_text(a.sql.clone());
            }
        });
    });
    ui.add_space(6.0);
    conexao::bloco_sql(ui, &a.tipo, &a.sql, 180.0, &format!("sql-gaveta-{}", a.id));
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label(tema::texto_forte("O que o agente vai receber", 13.0).color(p.texto));
        if let Some(Rodada::Resultado { r, hora }) = &v.rodada {
            let n = r.linhas.len();
            let linhas = if n == 1 { "1 linha".to_string() } else { format!("{n} linhas") };
            ui.label(RichText::new(format!("você rodou às {hora} · {linhas} · {} ms", r.ms)).color(p.suave).size(12.0));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let rotulo = if v.rodando {
                "Rodando…"
            } else if v.rodada.is_some() {
                "Rodar de novo"
            } else {
                "Rodar para mim antes"
            };
            if tema::botao_secundario_com(ui, rotulo, !v.rodando)
                .on_hover_text("Roda esta instrução no seu console, só leitura. O resultado fica só com você e entra no histórico como seu.")
                .clicked()
            {
                pedido = Some(Pedido::Rodar);
            }
        });
    });
    ui.add_space(6.0);
    match &v.rodada {
        Some(Rodada::Resultado { r, .. }) => tabela(ui, r),
        Some(Rodada::Erro(e)) => {
            egui::Frame::new()
                .fill(tema::fundo_tingido(p, p.erro, tema::claro()))
                .corner_radius(CornerRadius::same(tema::RAIO_CONTROLE))
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(*e).color(p.texto).size(12.5));
                });
        }
        None => {
            let texto = format!("Rode no seu console de {} para ver antes o que o agente vai receber. O resultado fica só com você.", a.conexao);
            let largura = ui.available_width();
            let g = ui.painter().layout(texto, FontId::proportional(12.5), p.suave, largura - 32.0);
            let (r, _) = ui.allocate_exact_size(vec2(largura, g.size().y + 24.0), Sense::hover());
            tracejado(ui.painter(), r, p.borda);
            ui.painter().galley(r.min + vec2(16.0, 12.0), g, p.suave);
        }
    }
    ui.add_space(14.0);
    ui.label(RichText::new("Antes, nesta conexão").color(p.suave).size(12.0));
    ui.add_space(2.0);
    match &v.antes {
        None => {
            ui.label(RichText::new("Carregando…").color(p.suave).size(12.5));
        }
        Some(lista) if lista.is_empty() => {
            ui.label(RichText::new("Este agente ainda não pediu nada aqui.").color(p.suave).size(12.5));
        }
        Some(lista) => {
            for c in lista {
                anterior(ui, &a.tipo, c);
            }
        }
    }
    pedido
}

/// Uma linha dos pedidos anteriores: o desfecho, a hora, o SQL e o que deu.
fn anterior(ui: &mut egui::Ui, dialeto: &str, c: &api::ConsultaFeita) {
    let p = cores();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let pintor = ui.painter();
    let centro = pos2(rect.left() + 8.0, rect.center().y);
    let info = match c.resultado.as_str() {
        "recusada" => {
            pintor.circle_stroke(centro, 3.0, Stroke::new(1.5, p.suave));
            "você recusou".to_string()
        }
        "expirou" => {
            pintor.circle_stroke(centro, 3.0, Stroke::new(1.5, p.alerta));
            "expirou".to_string()
        }
        "cancelada" => {
            pintor.circle_stroke(centro, 3.0, Stroke::new(1.5, p.suave));
            "cancelada".to_string()
        }
        _ if c.erro => {
            pintor.circle_filled(centro, 3.5, p.erro);
            "aprovado · deu erro".to_string()
        }
        _ => {
            pintor.circle_filled(centro, 3.5, p.ok);
            if c.linhas == 1 { "aprovado · 1 linha".to_string() } else { format!("aprovado · {} linhas", c.linhas) }
        }
    };
    let hora = pintor.layout_no_wrap(c.hora.clone(), FontId::monospace(11.5), p.suave);
    let x = rect.left() + 20.0;
    pintor.galley(pos2(x, rect.center().y - hora.size().y / 2.0), hora.clone(), p.suave);
    let g_info = pintor.layout_no_wrap(info, FontId::proportional(11.5), p.suave);
    pintor.galley(pos2(rect.right() - 4.0 - g_info.size().x, rect.center().y - g_info.size().y / 2.0), g_info.clone(), p.suave);
    let x_sql = x + hora.size().x + 12.0;
    let primeira = c.sql.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    let mut job = sql::realce(dialeto, &primeira, FontId::monospace(12.0), p, None);
    job.wrap = egui::text::TextWrapping {
        max_width: (rect.right() - 16.0 - g_info.size().x - x_sql).max(20.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let g = pintor.layout_job(job);
    pintor.galley(pos2(x_sql, rect.center().y - g.size().y / 2.0), g, p.texto);
}

/// O resultado de "Rodar para mim antes", compacto: só as linhas que
/// aparecem na rolagem são desenhadas.
fn tabela(ui: &mut egui::Ui, r: &api::Resultado) {
    let p = cores();
    if r.colunas.is_empty() {
        ui.label(RichText::new("A instrução não devolveu colunas.").color(p.suave).size(12.5));
        return;
    }
    let largura = ui.available_width();
    let largura_col = (largura / r.colunas.len() as f32).max(110.0);
    let total = largura_col * r.colunas.len() as f32;
    let altura = (LINHA_TABELA * (r.linhas.len().max(1) as f32 + 1.4)).min(240.0);
    let (moldura, _) = ui.allocate_exact_size(vec2(largura, altura), Sense::hover());
    ui.painter().rect(moldura, CornerRadius::same(tema::RAIO_CONTROLE), p.fundo, Stroke::new(1.0, p.borda), StrokeKind::Inside);
    let mut filho = ui.new_child(UiBuilder::new().max_rect(moldura.shrink(1.0)));
    egui::ScrollArea::both().id_salt("tabela-gaveta").auto_shrink(false).show_viewport(&mut filho, |ui, janela| {
        let alto = LINHA_TABELA * 1.4 + r.linhas.len() as f32 * LINHA_TABELA;
        let (rect, _) = ui.allocate_exact_size(vec2(total.max(janela.width()), alto), Sense::hover());
        let pintor = ui.painter();
        let cab = Rect::from_min_size(rect.min, vec2(rect.width(), LINHA_TABELA * 1.4));
        pintor.rect_filled(cab, CornerRadius::ZERO, p.superficie);
        for (c, col) in r.colunas.iter().enumerate() {
            let x = rect.left() + c as f32 * largura_col + 10.0;
            let nome = tema::cortar(pintor, &col.nome, egui::TextFormat::simple(forte(12.0), p.texto), largura_col - 20.0, 1, true);
            pintor.galley(pos2(x, cab.top() + 4.0), nome.clone(), p.texto);
            let tipo = tema::cortar(
                pintor,
                &super::arvore::tipo_curto(&col.tipo),
                egui::TextFormat::simple(FontId::monospace(10.0), p.suave),
                largura_col - 20.0,
                1,
                true,
            );
            pintor.galley(pos2(x, cab.top() + 4.0 + nome.size().y), tipo, p.suave);
        }
        let primeira = ((janela.min.y - cab.height()) / LINHA_TABELA).floor().max(0.0) as usize;
        let ultima = (((janela.max.y - cab.height()) / LINHA_TABELA).ceil() as usize + 1).min(r.linhas.len());
        for l in primeira..ultima {
            let y = cab.bottom() + l as f32 * LINHA_TABELA;
            if l % 2 == 1 {
                pintor.rect_filled(
                    Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), LINHA_TABELA)),
                    CornerRadius::ZERO,
                    p.realce.gamma_multiply(0.5),
                );
            }
            for c in 0..r.colunas.len() {
                let x = rect.left() + c as f32 * largura_col + 10.0;
                let (texto, cor, italico) = match r.linhas[l].get(c).cloned().flatten() {
                    None => ("NULL".to_string(), p.suave, true),
                    Some(v) => (v.replace(['\n', '\t'], " "), p.texto, false),
                };
                let formato = egui::TextFormat { font_id: FontId::monospace(12.0), color: cor, italics: italico, ..Default::default() };
                let g = tema::cortar(pintor, &texto, formato, largura_col - 20.0, 1, true);
                pintor.galley(pos2(x, y + (LINHA_TABELA - g.size().y) / 2.0), g, cor);
            }
        }
        if r.linhas.is_empty() {
            pintor.text(pos2(rect.left() + 10.0, cab.bottom() + LINHA_TABELA / 2.0), Align2::LEFT_CENTER, "Nenhuma linha", FontId::proportional(12.5), p.suave);
        }
    });
}

/// Moldura tracejada (o lugar do resultado antes de rodar).
fn tracejado(pintor: &egui::Painter, r: Rect, cor: Color32) {
    let traco = Stroke::new(1.0, cor);
    let lados = [(r.left_top(), r.right_top()), (r.right_top(), r.right_bottom()), (r.right_bottom(), r.left_bottom()), (r.left_bottom(), r.left_top())];
    for (a, b) in lados {
        pintor.extend(egui::Shape::dashed_line(&[a, b], traco, 5.0, 4.0));
    }
}

fn rodape(ui: &mut egui::Ui, v: &Vista, estado: &mut Estado) -> Option<Pedido> {
    let p = cores();
    let a = v.a;
    let mut pedido = None;
    if a.precisa_senha {
        ui.label(RichText::new(format!("Senha de {} (não está guardada)", a.conexao)).color(p.texto).size(12.5));
        let campo = conexao::campo_senha(ui, &mut estado.senha, "a senha da conexão", Id::new(("senha-gaveta", &a.id)));
        if std::mem::take(&mut estado.focar_senha) {
            campo.request_focus();
        }
        if campo.changed() {
            estado.erro_senha = None;
        }
        if let Some(e) = &estado.erro_senha {
            ui.label(RichText::new(e).color(p.erro).size(12.5));
        }
        if v.chaveiro {
            tema::caixa_marcar(ui, "Guardar no chaveiro", &mut estado.guardar);
        }
        ui.add_space(6.0);
    }
    if let Some(e) = &estado.erro {
        ui.label(RichText::new(e).color(p.erro).size(12.5));
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rotulo, pode) = if a.precisa_senha { ("Aprovar com esta senha", !estado.senha.is_empty()) } else { ("Aprovar e enviar", true) };
        let rotulo = if estado.enviando { "Enviando…" } else { rotulo };
        let largura_botoes = ui.painter().layout_no_wrap(rotulo.into(), FontId::proportional(13.0), p.texto).size().x + 120.0;
        let largura = (ui.available_width() - largura_botoes - 16.0).max(120.0);
        egui::Frame::new()
            .fill(tema::fundo_campo(p, false))
            .stroke(Stroke::new(1.0, p.borda))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(egui::Margin::symmetric(12, 7))
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut estado.motivo)
                        .id(Id::new(("motivo-gaveta", &a.id)))
                        .frame(egui::Frame::NONE)
                        .desired_width(largura - 24.0)
                        .char_limit(500)
                        .font(FontId::proportional(13.0))
                        .hint_text(RichText::new("Dizer ao agente por quê (opcional)").color(p.suave)),
                );
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tema::botao_principal(ui, rotulo, pode && !estado.enviando).clicked() {
                pedido = Some(Pedido::Responder(Resposta::Aprovar { senha: estado.senha.clone(), guardar: estado.guardar }));
            }
            if tema::botao_secundario_com(ui, "Recusar", !estado.enviando).clicked() {
                pedido = Some(Pedido::Responder(Resposta::Recusar { motivo: estado.motivo.trim().to_string() }));
            }
        });
    });
    ui.add_space(6.0);
    ui.label(RichText::new("O resultado vai para o agente e para o provedor de IA dele.").color(p.suave).size(11.5));
    pedido
}
