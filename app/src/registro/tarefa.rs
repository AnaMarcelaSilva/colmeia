//! A aba Linha do tempo da tarefa: "O que aconteceu" do Histórico, só com
//! os eventos dela (o núcleo filtra), com O que importa | Tudo e as
//! capturas no visor. Buscada ao aparecer depois de eventos novos; nada por
//! tempo. Os dias mais antigos vêm por um botão.

use eframe::egui::{self, Sense, pos2, vec2};

use super::comum::{self, em_segundo_plano};
use super::{Acao, Registro, Resposta, linha};
use crate::api;
use crate::tema::{self, cores, forte};

const DIA: f32 = 26.0;

pub(super) struct LinhaDaTarefa {
    pub tarefa: i64,
    perfil: i64,
    pub dias: Vec<api::Dia>,
    pub proximo: i64,
    pub carregando: bool,
    pub erro: Option<String>,
    /// Chegou evento com a aba fora da tela: busca ao voltar.
    suja: bool,
}

impl Registro {
    /// Algo entrou na linha do tempo: a aba da tarefa busca de novo quando aparecer.
    pub fn tarefa_mudou(&mut self) {
        if let Some(l) = &mut self.da_tarefa {
            l.suja = true;
        }
    }

    fn pedir_da_tarefa(&mut self, ctx: &egui::Context, primeira: bool) {
        let Some(l) = &mut self.da_tarefa else { return };
        if l.carregando && !primeira {
            return;
        }
        l.carregando = true;
        let (perfil, tarefa, antes) = (l.perfil, l.tarefa, if primeira { 0 } else { l.proximo });
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::PaginaTarefa { tarefa, primeira, resultado: api::linha_da_tarefa(perfil, tarefa, antes) });
    }

    pub(super) fn receber_da_tarefa(&mut self, tarefa: i64, primeira: bool, resultado: Result<api::PaginaLinha, String>) {
        let Some(l) = self.da_tarefa.as_mut().filter(|l| l.tarefa == tarefa) else { return };
        l.carregando = false;
        match resultado {
            Ok(pagina) => {
                l.erro = None;
                l.proximo = pagina.proximo;
                if primeira {
                    l.dias = pagina.dias;
                } else {
                    for dia in pagina.dias {
                        match l.dias.last_mut() {
                            Some(ultimo) if ultimo.dia == dia.dia => ultimo.itens.extend(dia.itens),
                            _ => l.dias.push(dia),
                        }
                    }
                }
            }
            Err(e) => l.erro = Some(e),
        }
    }

    /// Desenha a linha do tempo da tarefa na `ui`.
    pub fn mostrar_da_tarefa(&mut self, ui: &mut egui::Ui, perfil: i64, tarefa: i64) -> Vec<Acao> {
        let ctx = ui.ctx().clone();
        let p = cores();
        let mut acoes = Vec::new();
        if self.da_tarefa.as_ref().is_none_or(|l| l.tarefa != tarefa || l.perfil != perfil) {
            self.da_tarefa = Some(LinhaDaTarefa { tarefa, perfil, dias: Vec::new(), proximo: 0, carregando: false, erro: None, suja: true });
        }
        if self.conectado && self.da_tarefa.as_mut().is_some_and(|l| std::mem::take(&mut l.suja)) {
            self.pedir_da_tarefa(&ctx, true);
        }
        self.receber(&ctx, &mut acoes);
        let area = ui.available_rect_before_wrap();
        let largura = area.width().min(900.0);
        let coluna = egui::Rect::from_min_size(pos2(area.center().x - largura / 2.0, area.top() + 4.0), vec2(largura, area.height() - 4.0));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(coluna).layout(egui::Layout::top_down(egui::Align::Min)));
        let Some(l) = &self.da_tarefa else { return acoes };
        let dias: Vec<(&str, linha::DiaDaTarefa)> = l.dias.iter().filter_map(|d| linha::da_tarefa(d, tarefa).map(|t| (d.titulo.as_str(), t))).collect();
        let so_importa = self.so_importa;
        let visiveis: usize = dias.iter().map(|(_, d)| d.eventos.iter().filter(|e| !so_importa || e.importa()).count()).sum();
        let mut trocar = None;
        ui.horizontal(|ui| {
            ui.label(tema::texto_forte("O que aconteceu", 14.5).color(p.texto));
            ui.label(egui::RichText::new(if visiveis == 1 { "1 evento".to_string() } else { format!("{visiveis} eventos") }).size(12.0).color(p.suave));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                trocar = tema::segmentado(ui, &["O que importa", "Tudo"], if so_importa { 0 } else { 1 });
            });
        });
        ui.add_space(8.0);
        let mut visor = None;
        let mut mais = false;
        if let Some(e) = &l.erro {
            ui.label(egui::RichText::new(format!("Não consegui ler a linha do tempo: {e}")).color(p.erro));
        } else if dias.is_empty() && l.carregando {
            tema::esqueleto(ui.painter(), egui::Rect::from_min_size(ui.cursor().min, vec2(largura * 0.6, 14.0)), tema::RAIO_ETIQUETA);
        } else if visiveis == 0 {
            ui.label(egui::RichText::new("Nada por aqui ainda.").color(p.suave));
        }
        let proximo = l.proximo;
        egui::ScrollArea::vertical().id_salt(("linha-da-tarefa", tarefa)).auto_shrink(false).show(&mut ui, |ui| {
            let largura = ui.available_width() - comum::MARGEM_ROLAGEM;
            for (titulo, d) in &dias {
                let altura = d.altura(so_importa);
                if altura == 0.0 {
                    continue;
                }
                let (rect, _) = ui.allocate_exact_size(vec2(largura, DIA + altura), Sense::hover());
                ui.painter().text(pos2(rect.left() + 8.0, rect.top() + 13.0), egui::Align2::LEFT_CENTER, *titulo, forte(11.5), p.suave);
                if let Some(n) = linha::desenhar(ui, pos2(rect.left() + 8.0, rect.top() + DIA), largura - 8.0, d, so_importa, &mut self.miniaturas) {
                    let a = &d.anexos[n];
                    visor = Some((a.id, a.texto.to_string(), a.hora.to_string()));
                }
                ui.add_space(6.0);
            }
            if proximo > 0 {
                ui.add_space(4.0);
                mais = tema::botao_secundario(ui, "Ver os dias mais antigos").clicked();
            }
        });
        if let Some(t) = trocar {
            self.so_importa = t == 0;
        }
        if mais {
            self.pedir_da_tarefa(&ctx, false);
        }
        if let Some((anexo, texto, hora)) = visor {
            self.abrir_visor(&ctx, anexo, &texto, &hora, tarefa);
        }
        self.mostrar_visor(&ctx, &mut acoes);
        acoes
    }
}
