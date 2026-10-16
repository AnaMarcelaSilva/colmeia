//! Os eventos de uma tarefa num dia, do jeito que a linha do tempo do núcleo
//! manda: as repetições seguidas viram uma linha com "(N vezes)" e as
//! capturas viram miniaturas. O Histórico usa para "O que aconteceu".

use eframe::egui::{self, FontId, Id, Pos2, Rect, Sense, Stroke, pos2, vec2};

use super::comum::{self, desenhar_miniatura, texto_cortado};
use crate::api;
use crate::tema::{self, cores};

pub(super) const EVENTO: f32 = 26.0;
pub(super) const MINIATURAS: f32 = 10.0 + 64.0;
const MAX_MINIATURAS: usize = 6;

/// O que entra em "O que importa": o caminho da tarefa, o que pediu você e
/// o que deu errado (as sessões e os pedidos ficam só em "Tudo").
const IMPORTANTES: [&str; 9] = ["criou", "moveu", "concluiu", "erro", "interrompido", "sessao_aguardando", "nota", "nota_agente", "pedido_respondido"];

/// Um evento (as repetições seguidas juntas).
pub(super) struct Evento<'a> {
    pub hora: &'a str,
    pub tipo: &'a str,
    texto: &'a str,
    vezes: usize,
}

impl Evento<'_> {
    /// A frase com a contagem dentro: "Anotou na daily (2 vezes)."
    fn frase(&self) -> std::borrow::Cow<'_, str> {
        if self.vezes > 1 { format!("{} ({} vezes).", self.texto.trim_end_matches('.'), self.vezes).into() } else { self.texto.into() }
    }

    pub fn importa(&self) -> bool {
        IMPORTANTES.contains(&self.tipo)
    }
}

/// Uma miniatura, com a legenda e a hora do item de onde veio (o visor
/// mostra as do anexo clicado).
pub(super) struct Anexo<'a> {
    pub id: i64,
    video: bool,
    pub texto: &'a str,
    pub hora: &'a str,
}

/// O dia de uma tarefa: os eventos (do mais novo ao mais antigo) e as capturas.
pub(super) struct DiaDaTarefa<'a> {
    pub eventos: Vec<Evento<'a>>,
    pub anexos: Vec<Anexo<'a>>,
}

impl DiaDaTarefa<'_> {
    pub fn altura(&self, so_importa: bool) -> f32 {
        let n = self.eventos.iter().filter(|e| !so_importa || e.importa()).count();
        let imagens = if self.anexos.is_empty() || so_importa { 0.0 } else { MINIATURAS };
        n as f32 * EVENTO + imagens
    }
}

/// Os itens da tarefa num dia; None quando ela não aparece nele. Um evento
/// de outra tarefa no meio não separa as repetições.
pub(super) fn da_tarefa(dia: &api::Dia, tarefa: i64) -> Option<DiaDaTarefa<'_>> {
    let mut d = DiaDaTarefa { eventos: Vec::new(), anexos: Vec::new() };
    for item in dia.itens.iter().filter(|i| i.tarefa_id == tarefa) {
        for &id in &item.anexos {
            d.anexos.push(Anexo { id, video: item.videos.contains(&id), texto: &item.texto, hora: &item.hora });
        }
        let texto = if item.curto.is_empty() { &item.texto } else { &item.curto };
        match d.eventos.last_mut() {
            Some(e) if e.texto == texto && e.tipo == item.tipo => e.vezes += 1,
            _ => d.eventos.push(Evento { hora: &item.hora, tipo: &item.tipo, texto, vezes: 1 }),
        }
    }
    (!d.eventos.is_empty()).then_some(d)
}

/// Desenha os eventos do dia a partir de `pos` (hora, ponto num fio e
/// frase) e as miniaturas embaixo. Devolve a miniatura clicada.
pub(super) fn desenhar(ui: &mut egui::Ui, pos: Pos2, largura: f32, d: &DiaDaTarefa, so_importa: bool, cache: &mut comum::CacheImagens) -> Option<usize> {
    let p = cores();
    let pintor = ui.painter().clone();
    let eventos: Vec<&Evento> = d.eventos.iter().filter(|e| !so_importa || e.importa()).collect();
    let x_ponto = pos.x + 50.0;
    // O fio que liga os pontos do dia.
    if eventos.len() > 1 {
        let (topo, base) = (pos.y + EVENTO / 2.0, pos.y + EVENTO * (eventos.len() as f32 - 0.5));
        pintor.line_segment([pos2(x_ponto, topo), pos2(x_ponto, base)], Stroke::new(1.0, p.borda));
    }
    let mut y = pos.y;
    for e in &eventos {
        let meio = y + EVENTO / 2.0;
        pintor.text(pos2(pos.x, meio), egui::Align2::LEFT_CENTER, e.hora, FontId::proportional(12.5), p.suave);
        pintor.circle_filled(pos2(x_ponto, meio), 5.0, p.superficie_alta);
        tema::ponto(&pintor, pos2(x_ponto, meio), if e.tipo == "concluiu" { 4.0 } else { 3.0 }, comum::ponto_do_tipo(e.tipo));
        let x = pos.x + 62.0;
        let fim = pos.x + largura;
        let frase = e.frase();
        let (fonte, cor) = if e.importa() { (FontId::proportional(13.5), p.texto) } else { (FontId::proportional(13.0), p.suave) };
        let fonte = if e.tipo == "concluiu" { tema::forte(13.5) } else { fonte };
        if texto_cortado(&pintor, pos2(x, meio), &frase, fonte, cor, fim - x) {
            ui.interact(Rect::from_min_max(pos2(x, y), pos2(fim, y + EVENTO)), Id::new(("evento-historico", pos.y as i32, y as i32)), Sense::hover())
                .on_hover_text(frase.as_ref());
        }
        y += EVENTO;
    }
    let mut clicada = None;
    if !d.anexos.is_empty() && !so_importa {
        let mut x = pos.x + 62.0;
        let y = y + 10.0;
        for (n, anexo) in d.anexos.iter().enumerate() {
            let caixa = Rect::from_min_size(pos2(x, y), vec2(64.0, 64.0));
            if n == MAX_MINIATURAS {
                comum::caixa_mais(&pintor, caixa, d.anexos.len() - MAX_MINIATURAS);
                break;
            }
            if anexo.video {
                comum::caixa_video(ui, caixa, Id::new(("video-historico", anexo.id)), tema::RAIO_CONTROLE)
                    .on_hover_text("Vídeo: abra na apresentação da daily ou da sprint");
            } else if desenhar_miniatura(ui, cache, caixa, anexo.id, tema::RAIO_CONTROLE).on_hover_text(format!("{} · {}", anexo.texto, anexo.hora)).clicked()
            {
                clicada = Some(n);
            }
            x += 64.0 + 8.0;
        }
    }
    clicada
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(hora: &str, tipo: &str, tarefa: i64, curto: &str) -> api::ItemLinha {
        serde_json::from_value(serde_json::json!({
            "evento": 1, "hora": hora, "tipo": tipo, "texto": format!("{curto} em “T{tarefa}”"), "curto": curto,
            "tarefa_id": tarefa, "titulo": format!("T{tarefa}"), "coluna": "trabalhando"
        }))
        .unwrap()
    }

    #[test]
    fn junta_repeticoes_da_tarefa() {
        let mut dia: api::Dia = serde_json::from_value(serde_json::json!({"dia": "2026-10-02", "titulo": "Hoje", "itens": []})).unwrap();
        dia.itens = vec![
            item("10:05", "sessao", 1, "Claude Code (dev) trabalhou menos de 1 min."),
            item("10:04", "sessao", 1, "Claude Code (dev) trabalhou menos de 1 min."),
            item("10:03", "erro", 2, "Codex parou com erro."),
            item("10:02", "sessao", 1, "Claude Code (dev) trabalhou menos de 1 min."),
            item("10:01", "concluiu", 1, "Concluída."),
        ];
        let d = da_tarefa(&dia, 1).unwrap();
        // O evento de outra tarefa no meio não separa as repetições.
        let eventos: Vec<(usize, &str)> = d.eventos.iter().map(|e| (e.vezes, e.texto)).collect();
        assert_eq!(eventos, vec![(3, "Claude Code (dev) trabalhou menos de 1 min."), (1, "Concluída.")]);
        assert_eq!(d.eventos[0].frase(), "Claude Code (dev) trabalhou menos de 1 min (3 vezes).");
        assert!(!d.eventos[0].importa() && d.eventos[1].importa());
        assert_eq!(d.altura(false), 2.0 * EVENTO);
        assert_eq!(d.altura(true), EVENTO);
        assert!(da_tarefa(&dia, 3).is_none());
    }
}
