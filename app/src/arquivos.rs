//! A aba Arquivos da tarefa, só leitura: à esquerda a busca por nome
//! (Ctrl+P) sobre a árvore da pasta ou a lista do que mudou; no meio o
//! arquivo aberto ou o diff dele contra a branch base; à direita a busca por
//! texto no código (Ctrl+Shift+F), com a linha e o trecho. Para editar, o
//! botão abre no editor (IntelliJ), na linha.
//!
//! A busca, o diff e o que mudou vêm prontos do núcleo, que confina tudo à
//! pasta da tarefa e corta os resultados. Nada roda por tempo: cada pedido
//! sai de uma ação (digitar, Enter, abrir a aba) numa thread.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Align2, CornerRadius, FontId, Id, Key, Modifiers, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::api;
use crate::gaveta::{self, PedidoGaveta, cor_da_mudanca};
use crate::registro::{self, em_segundo_plano, texto_cortado};
use crate::tema::{self, cores, forte};

const LINHA: f32 = 26.0;
const LINHA_ACHADO: f32 = 22.0;
const LINHA_DIFF: f32 = 18.0;

enum Estado<T> {
    Nada,
    Carregando,
    Pronto(T),
    Erro(String),
}

enum Resposta {
    Nomes(String, Result<api::BuscaNome, String>),
    Texto(String, Result<api::BuscaTexto, String>),
    Mudancas(Result<api::Mudancas, String>),
    Diff(String, Result<api::Diferenca, String>),
}

/// Uma fileira da busca por texto: o arquivo (cabeçalho do grupo) ou uma linha achada.
enum Fileira {
    Arquivo(usize),
    Achado(usize),
}

pub struct AbaArquivos {
    tarefa: i64,
    arvore: gaveta::Arvore,
    nome: String,
    nomes: Estado<api::BuscaNome>,
    texto: String,
    maiusculas: bool,
    texto_pedido: String,
    achados: Estado<api::BuscaTexto>,
    mudancas: Estado<api::Mudancas>,
    mudadas: HashMap<String, api::Mudanca>,
    /// A coluna da esquerda mostra só o que mudou (em vez da árvore).
    so_mudou: bool,
    /// O meio mostra o diff do arquivo aberto (em vez do arquivo).
    ver_diff: bool,
    diff: Option<(String, Estado<api::Diferenca>)>,
    focar_nome: bool,
    focar_texto: bool,
    canal: (Sender<Resposta>, Receiver<Resposta>),
}

impl AbaArquivos {
    pub fn nova(ctx: &egui::Context, tarefa: i64) -> AbaArquivos {
        let mut a = AbaArquivos {
            tarefa,
            arvore: gaveta::Arvore::nova(ctx, tarefa),
            nome: String::new(),
            nomes: Estado::Nada,
            texto: String::new(),
            maiusculas: false,
            texto_pedido: String::new(),
            achados: Estado::Nada,
            mudancas: Estado::Nada,
            mudadas: HashMap::new(),
            so_mudou: false,
            ver_diff: false,
            diff: None,
            focar_nome: false,
            focar_texto: false,
            canal: mpsc::channel(),
        };
        a.pedir_mudancas(ctx);
        a
    }

    /// Ao voltar para a aba: o agente pode ter mudado arquivos desde a última vez.
    pub fn ao_abrir(&mut self, ctx: &egui::Context) {
        self.pedir_mudancas(ctx);
        if self.ver_diff
            && let Some(caminho) = self.arvore.aberto().map(str::to_string)
        {
            self.pedir_diff(ctx, &caminho, false);
        }
    }

    pub fn focar_nome(&mut self) {
        self.focar_nome = true;
    }

    pub fn focar_texto(&mut self) {
        self.focar_texto = true;
    }

    /// Quantos arquivos mudaram contra a base (o número da aba), se já se sabe.
    pub fn quantas_mudaram(&self) -> Option<usize> {
        match &self.mudancas {
            Estado::Pronto(m) => Some(m.arquivos.len()),
            _ => None,
        }
    }

    fn pedir_mudancas(&mut self, ctx: &egui::Context) {
        if !matches!(self.mudancas, Estado::Pronto(_)) {
            self.mudancas = Estado::Carregando;
        }
        let tarefa = self.tarefa;
        em_segundo_plano(&self.canal.0, ctx, move || Resposta::Mudancas(api::mudancas(tarefa)));
    }

    fn pedir_nomes(&mut self, ctx: &egui::Context) {
        let (tarefa, termo) = (self.tarefa, self.nome.trim().to_string());
        if termo.is_empty() {
            self.nomes = Estado::Nada;
            return;
        }
        if !matches!(self.nomes, Estado::Pronto(_)) {
            self.nomes = Estado::Carregando;
        }
        em_segundo_plano(&self.canal.0, ctx, move || {
            let r = api::buscar_nome(tarefa, &termo);
            Resposta::Nomes(termo, r)
        });
    }

    fn pedir_texto(&mut self, ctx: &egui::Context) {
        let (tarefa, termo, maiusculas) = (self.tarefa, self.texto.clone(), self.maiusculas);
        if termo.trim().is_empty() {
            self.achados = Estado::Nada;
            return;
        }
        self.texto_pedido = termo.clone();
        self.achados = Estado::Carregando;
        em_segundo_plano(&self.canal.0, ctx, move || {
            let r = api::buscar_texto(tarefa, &termo, maiusculas);
            Resposta::Texto(termo, r)
        });
    }

    fn pedir_diff(&mut self, ctx: &egui::Context, caminho: &str, mostrar: bool) {
        self.diff = Some((caminho.to_string(), Estado::Carregando));
        let (tarefa, caminho) = (self.tarefa, caminho.to_string());
        em_segundo_plano(&self.canal.0, ctx, move || {
            let r = api::ver_mudanca(tarefa, &caminho, mostrar);
            Resposta::Diff(caminho, r)
        });
    }

    fn receber(&mut self) {
        while let Ok(r) = self.canal.1.try_recv() {
            match r {
                // Só vale a resposta do termo que está no campo.
                Resposta::Nomes(termo, r) if termo == self.nome.trim() => {
                    self.nomes = match r {
                        Ok(n) => Estado::Pronto(n),
                        Err(e) => Estado::Erro(e),
                    }
                }
                Resposta::Texto(termo, r) if termo == self.texto_pedido => {
                    self.achados = match r {
                        Ok(a) => Estado::Pronto(a),
                        Err(e) => Estado::Erro(e),
                    }
                }
                Resposta::Mudancas(r) => match r {
                    Ok(m) => {
                        self.mudadas = m.arquivos.iter().map(|a| (a.caminho.clone(), a.clone())).collect();
                        self.mudancas = Estado::Pronto(m);
                    }
                    Err(e) => {
                        self.mudadas.clear();
                        self.mudancas = Estado::Erro(e);
                    }
                },
                Resposta::Diff(caminho, r) => {
                    if let Some((c, estado)) = &mut self.diff
                        && *c == caminho
                    {
                        *estado = match r {
                            Ok(d) => Estado::Pronto(d),
                            Err(e) => Estado::Erro(e),
                        };
                    }
                }
                _ => {}
            }
        }
    }

    /// Abre um arquivo no meio: no diff, se `diff`, ou no texto (na linha).
    fn abrir(&mut self, ctx: &egui::Context, caminho: &str, linha: Option<usize>, diff: bool) {
        self.arvore.abrir_na_linha(ctx, caminho, linha, false);
        self.ver_diff = diff;
        if diff {
            self.pedir_diff(ctx, caminho, false);
        }
    }

    /// Desenha a aba em `area`. `editor`: o nome do editor, se houver.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, area: Rect, pasta: &str, editor: Option<&str>) -> Option<PedidoGaveta> {
        self.receber();
        let lado = (area.width() * 0.24).clamp(240.0, 320.0);
        let vao = 12.0;
        let esquerda = Rect::from_min_size(area.min, vec2(lado, area.height()));
        let direita = Rect::from_min_max(pos2(area.right() - lado, area.top()), area.max);
        let meio = Rect::from_min_max(pos2(esquerda.right() + vao, area.top()), pos2(direita.left() - vao, area.bottom()));
        let mut pedido = None;
        for (coluna, f) in [(esquerda, 0), (meio, 1), (direita, 2)] {
            painel(ui, coluna);
            let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(coluna.shrink(10.0)).layout(egui::Layout::top_down(egui::Align::Min)));
            filho.set_clip_rect(coluna.shrink(2.0));
            let pg = match f {
                0 => self.coluna_esquerda(&mut filho, pasta),
                1 => self.coluna_meio(&mut filho, editor),
                _ => {
                    self.coluna_busca(&mut filho);
                    None
                }
            };
            if pg.is_some() {
                pedido = pg;
            }
        }
        pedido
    }

    fn coluna_esquerda(&mut self, ui: &mut egui::Ui, pasta: &str) -> Option<PedidoGaveta> {
        let ctx = ui.ctx().clone();
        let id = Id::new(("busca-nome", self.tarefa));
        let campo = ui.add(
            egui::TextEdit::singleline(&mut self.nome)
                .id(id)
                .hint_text("Buscar arquivo  Ctrl+P")
                .desired_width(ui.available_width())
                .margin(egui::Margin::symmetric(8, 6)),
        );
        if std::mem::take(&mut self.focar_nome) {
            campo.request_focus();
        }
        if campo.changed() {
            self.pedir_nomes(&ctx);
        }
        let com_foco = campo.has_focus();
        if com_foco && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.nome.clear();
            self.nomes = Estado::Nada;
        }
        let enter = campo.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        ui.add_space(8.0);
        if !self.nome.trim().is_empty() {
            let primeiro = match &self.nomes {
                Estado::Pronto(n) => n.arquivos.first().map(|a| a.caminho.clone()),
                _ => None,
            };
            if enter && let Some(c) = primeiro {
                self.abrir(&ctx, &c, None, false);
                return None;
            }
            self.resultados_nome(ui);
            return None;
        }
        let n = self.quantas_mudaram().unwrap_or(0);
        let rotulo_mudou = format!("Mudou {n}");
        if let Some(i) = tema::segmentado_com_largura(ui, &["Pasta", &rotulo_mudou], usize::from(self.so_mudou), Some(ui.available_width())) {
            self.so_mudou = i == 1;
        }
        ui.add_space(6.0);
        if self.so_mudou {
            self.lista_mudancas(ui);
            return None;
        }
        let mudadas = std::mem::take(&mut self.mudadas);
        let pedido = self.arvore.mostrar_arvore(ui, pasta, &mudadas);
        self.mudadas = mudadas;
        // Abrir um arquivo pela árvore mostra o arquivo (não o diff).
        if self.ver_diff && self.diff.as_ref().map(|(c, _)| c.as_str()) != self.arvore.aberto() {
            self.ver_diff = false;
        }
        pedido
    }

    fn resultados_nome(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let nomes = match &self.nomes {
            Estado::Pronto(n) => n,
            Estado::Erro(e) => {
                aviso(ui, e, true);
                return;
            }
            _ => {
                tema::esqueleto(ui.painter(), Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 14.0)), tema::RAIO_ETIQUETA);
                return;
            }
        };
        if nomes.arquivos.is_empty() {
            aviso(ui, "Nenhum arquivo com esse nome.", false);
            return;
        }
        let mut abrir = None;
        egui::ScrollArea::vertical().id_salt(("nomes", self.tarefa)).auto_shrink(false).show_rows(ui, LINHA + 6.0, nomes.arquivos.len(), |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for a in &nomes.arquivos[faixa] {
                let largura = ui.available_width() - registro::MARGEM_ROLAGEM;
                let (rect, r) = ui.allocate_exact_size(vec2(largura, LINHA + 6.0), Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
                }
                let (pasta, nome) = a.caminho.rsplit_once('/').unwrap_or(("", &a.caminho));
                let marca = self.mudadas.get(&a.caminho).map(|m| m.estado.as_str());
                let fim = rect.right() - if marca.is_some() { 22.0 } else { 6.0 };
                texto_cortado(ui.painter(), pos2(rect.left() + 8.0, rect.top() + 10.0), nome, forte(13.0), p.texto, fim - rect.left() - 8.0);
                texto_cortado(ui.painter(), pos2(rect.left() + 8.0, rect.top() + 23.0), pasta, FontId::proportional(11.5), p.suave, fim - rect.left() - 8.0);
                if let Some(m) = marca {
                    ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, m, forte(11.5), cor_da_mudanca(m));
                }
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&a.caminho).clicked() {
                    abrir = Some(a.caminho.clone());
                }
            }
        });
        if nomes.mais {
            ui.label(egui::RichText::new("Mostrando os melhores; continue digitando.").size(11.5).color(p.suave));
        }
        if let Some(c) = abrir {
            self.abrir(&ctx, &c, None, false);
        }
    }

    fn lista_mudancas(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let m = match &self.mudancas {
            Estado::Pronto(m) => m,
            Estado::Erro(e) => {
                aviso(ui, e, true);
                return;
            }
            _ => {
                tema::esqueleto(ui.painter(), Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 14.0)), tema::RAIO_ETIQUETA);
                return;
            }
        };
        ui.label(egui::RichText::new(format!("Contra {}, com o que ainda não está em commit", m.base)).size(11.5).color(p.suave));
        ui.add_space(4.0);
        if m.arquivos.is_empty() {
            aviso(ui, "Nada mudou nesta tarefa ainda.", false);
            return;
        }
        let aberto = self.arvore.aberto().map(str::to_string);
        let mut abrir = None;
        egui::ScrollArea::vertical().id_salt(("mudou", self.tarefa)).auto_shrink(false).show_rows(ui, LINHA, m.arquivos.len(), |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for a in &m.arquivos[faixa] {
                let largura = ui.available_width() - registro::MARGEM_ROLAGEM;
                let (rect, r) = ui.allocate_exact_size(vec2(largura, LINHA), Sense::click());
                if aberto.as_deref() == Some(a.caminho.as_str()) {
                    ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_ETIQUETA), tema::fundo_escolhido());
                } else if r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
                }
                let y = rect.center().y;
                ui.painter().text(pos2(rect.left() + 8.0, y), Align2::LEFT_CENTER, &a.estado, forte(11.5), cor_da_mudanca(&a.estado));
                let contagem = if a.binario || (a.mais == 0 && a.menos == 0) { String::new() } else { format!("+{} −{}", a.mais, a.menos) };
                let g = ui.painter().layout_no_wrap(contagem, FontId::proportional(11.5), p.suave);
                let largura_contagem = g.size().x;
                ui.painter().galley(pos2(rect.right() - 6.0 - largura_contagem, y - g.size().y / 2.0), g, p.suave);
                let largura_nome = rect.width() - 30.0 - largura_contagem - 10.0;
                tema::texto_sem_inicio(ui.painter(), pos2(rect.left() + 24.0, y - 8.0), &a.caminho, FontId::proportional(13.0), p.texto, largura_nome);
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&a.caminho).clicked() {
                    abrir = Some((a.caminho.clone(), a.estado != "D"));
                }
            }
        });
        if m.mais {
            ui.label(egui::RichText::new("Mostrando os primeiros 500.").size(11.5).color(p.suave));
        }
        if let Some((c, existe)) = abrir {
            // Apagado: só o diff (não há arquivo para mostrar).
            self.abrir(&ctx, &c, None, true);
            if !existe {
                self.ver_diff = true;
            }
        }
    }

    fn coluna_meio(&mut self, ui: &mut egui::Ui, editor: Option<&str>) -> Option<PedidoGaveta> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some(caminho) = self.arvore.aberto().map(str::to_string) else {
            let texto = "Escolha um arquivo na árvore, busque pelo nome (Ctrl+P) ou por um texto do código (Ctrl+Shift+F).\nAqui é só leitura: para editar, abra no editor.";
            let r = ui.max_rect();
            let g = ui.painter().layout(texto.into(), FontId::proportional(13.5), p.suave, (r.width() - 40.0).min(420.0));
            ui.painter().galley(r.center() - g.size() / 2.0, g, p.suave);
            return None;
        };
        // A troca entre o arquivo e o diff, com o selo de só leitura.
        let mut trocar = None;
        ui.horizontal(|ui| {
            ui.set_height(34.0);
            if let Some(i) = tema::segmentado(ui, &["Arquivo", "Mudanças"], usize::from(self.ver_diff)) {
                trocar = Some(i == 1);
            }
            ui.add_space(8.0);
            if let Some(m) = self.mudadas.get(&caminho) {
                let texto = match m.estado.as_str() {
                    "A" => "novo nesta branch",
                    "D" => "apagado nesta branch",
                    _ => "mudou nesta branch",
                };
                ui.label(egui::RichText::new(texto).size(12.0).color(cor_da_mudanca(&m.estado)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                tema::etiqueta_ui(ui, "Só leitura", tema::fonte_etiqueta(), p.suave);
            });
        });
        match trocar {
            Some(true) if !self.ver_diff => {
                self.ver_diff = true;
                self.pedir_diff(&ctx, &caminho, false);
            }
            Some(false) => self.ver_diff = false,
            _ => {}
        }
        if self.ver_diff {
            // O diff que veio é de outro arquivo (a árvore abriu outro): pede o deste.
            if self.diff.as_ref().is_none_or(|(c, _)| *c != caminho) {
                self.pedir_diff(&ctx, &caminho, false);
            }
            return self.diff(ui, &caminho, editor);
        }
        self.arvore.mostrar_previa(ui, editor)
    }

    fn diff(&mut self, ui: &mut egui::Ui, caminho: &str, editor: Option<&str>) -> Option<PedidoGaveta> {
        let p = cores();
        let ctx = ui.ctx().clone();
        let Some((_, estado)) = &self.diff else { return None };
        let mut pedido = None;
        let mut mostrar = false;
        ui.add_space(6.0);
        match estado {
            Estado::Nada | Estado::Carregando => tema::esqueleto(ui.painter(), ui.available_rect_before_wrap(), tema::RAIO_CONTROLE),
            Estado::Erro(e) => aviso(ui, e, true),
            Estado::Pronto(d) => {
                let (mais, menos) = d.linhas.iter().fold((0, 0), |(a, b), l| (a + usize::from(l.tipo == "mais"), b + usize::from(l.tipo == "menos")));
                let primeira = d.linhas.iter().find(|l| l.tipo == "mais" || l.tipo == "menos").map(|l| l.nova.max(1));
                ui.horizontal(|ui| {
                    ui.set_height(30.0);
                    let resumo = match d.tipo.as_str() {
                        "diff" => format!("Contra {} · +{mais} −{menos}", d.base),
                        _ => format!("Contra {}", d.base),
                    };
                    ui.label(egui::RichText::new(resumo).size(12.5).color(p.suave));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(nome) = editor
                            && tema::botao_secundario_com(ui, &format!("Abrir no {nome}"), !d.caminho_absoluto.is_empty())
                                .on_hover_text("Abrir no editor, na primeira mudança")
                                .clicked()
                        {
                            pedido = Some(PedidoGaveta::AbrirNoEditor(d.caminho_absoluto.clone(), primeira));
                        }
                    });
                });
                ui.add_space(4.0);
                match d.tipo.as_str() {
                    "diff" => linhas_do_diff(ui, &d.linhas, d.cortado, caminho),
                    "igual" => aviso(ui, &format!("Este arquivo está igual em {}.", d.base), false),
                    "binario" => aviso(ui, "Arquivo binário: sem diff para mostrar.", false),
                    "sensivel" => {
                        aviso(ui, "Este arquivo pode ter senhas ou chaves, e a tela pode estar compartilhada.", false);
                        ui.add_space(8.0);
                        mostrar = tema::botao_secundario(ui, "Mostrar mesmo assim").clicked();
                    }
                    _ => {}
                }
            }
        }
        if mostrar {
            self.pedir_diff(&ctx, caminho, true);
        }
        pedido
    }

    fn coluna_busca(&mut self, ui: &mut egui::Ui) {
        let p = cores();
        let ctx = ui.ctx().clone();
        let id = Id::new(("busca-texto", self.tarefa));
        let mut buscar = false;
        ui.horizontal(|ui| {
            let largura = ui.available_width() - 60.0;
            let campo = ui.add(
                egui::TextEdit::singleline(&mut self.texto)
                    .id(id)
                    .hint_text("Buscar no código  Ctrl+Shift+F")
                    .desired_width(largura)
                    .margin(egui::Margin::symmetric(8, 6)),
            );
            if std::mem::take(&mut self.focar_texto) {
                campo.request_focus();
            }
            buscar = campo.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let aa = tema::chip_alternar(ui, "Aa", self.maiusculas).on_hover_text("Diferenciar maiúsculas");
            if aa.clicked() {
                self.maiusculas = !self.maiusculas;
                buscar = !self.texto.trim().is_empty();
            }
        });
        if buscar {
            self.pedir_texto(&ctx);
        }
        ui.add_space(8.0);
        let achados = match &self.achados {
            Estado::Nada => {
                aviso(ui, "Digite e aperte Enter. A busca olha os arquivos de texto da pasta, sem os ignorados pelo git nem os que costumam ter senha.", false);
                return;
            }
            Estado::Carregando => {
                tema::esqueleto(ui.painter(), Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), 14.0)), tema::RAIO_ETIQUETA);
                return;
            }
            Estado::Erro(e) => {
                aviso(ui, e, true);
                return;
            }
            Estado::Pronto(a) => a,
        };
        if achados.achados.is_empty() {
            aviso(ui, "Nada encontrado.", false);
            return;
        }
        let fileiras = agrupar(&achados.achados);
        let arquivos = fileiras.iter().filter(|f| matches!(f, Fileira::Arquivo(_))).count();
        let total = achados.achados.len();
        let resumo = format!(
            "{total}{} {} em {arquivos} {}",
            if achados.mais { "+" } else { "" },
            if total == 1 { "linha" } else { "linhas" },
            if arquivos == 1 { "arquivo" } else { "arquivos" }
        );
        ui.label(egui::RichText::new(resumo).size(11.5).color(p.suave));
        ui.add_space(4.0);
        let termo = self.texto_pedido.clone();
        let maiusculas = self.maiusculas;
        let mut abrir = None;
        egui::ScrollArea::vertical().id_salt(("achados", self.tarefa)).auto_shrink(false).show_rows(ui, LINHA_ACHADO, fileiras.len(), |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for f in &fileiras[faixa] {
                let largura = ui.available_width() - registro::MARGEM_ROLAGEM;
                match *f {
                    Fileira::Arquivo(i) => {
                        let (rect, _) = ui.allocate_exact_size(vec2(largura, LINHA_ACHADO), Sense::hover());
                        let c = &achados.achados[i].caminho;
                        tema::texto_sem_inicio(ui.painter(), pos2(rect.left() + 2.0, rect.center().y - 8.0), c, forte(12.5), p.texto, rect.width() - 4.0);
                    }
                    Fileira::Achado(i) => {
                        let a = &achados.achados[i];
                        let (rect, r) = ui.allocate_exact_size(vec2(largura, LINHA_ACHADO), Sense::click());
                        if r.hovered() {
                            ui.painter().rect_filled(rect, CornerRadius::same(tema::RAIO_ETIQUETA), p.realce);
                        }
                        let y = rect.center().y;
                        ui.painter().text(pos2(rect.left() + 34.0, y), Align2::RIGHT_CENTER, a.linha.to_string(), FontId::monospace(11.0), p.suave);
                        let job = trecho_marcado(&a.texto, &termo, maiusculas, rect.width() - 46.0);
                        let g = ui.painter().layout_job(job);
                        ui.painter().galley(pos2(rect.left() + 42.0, y - g.size().y / 2.0), g, p.texto);
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            abrir = Some((a.caminho.clone(), a.linha));
                        }
                    }
                }
            }
        });
        if achados.mais {
            ui.label(egui::RichText::new("Mostrando as primeiras 200 linhas.").size(11.5).color(p.suave));
        }
        if let Some((c, linha)) = abrir {
            self.abrir(&ctx, &c, Some(linha), false);
        }
    }
}

/// O fundo de cada coluna: um painel suave dentro do cartão.
fn painel(ui: &egui::Ui, r: Rect) {
    let p = cores();
    ui.painter().rect(r, CornerRadius::same(14), p.superficie, Stroke::new(1.0, p.borda.gamma_multiply(0.7)), StrokeKind::Inside);
}

/// Um aviso curto na coluna (o erro do núcleo em vermelho).
fn aviso(ui: &mut egui::Ui, texto: &str, erro: bool) {
    let p = cores();
    let cor = if erro { p.erro } else { p.suave };
    ui.add(egui::Label::new(egui::RichText::new(texto).size(12.5).color(cor)).wrap());
}

/// As linhas achadas agrupadas por arquivo, na ordem em que vieram.
fn agrupar(achados: &[api::Achado]) -> Vec<Fileira> {
    let mut fileiras = Vec::with_capacity(achados.len() + 8);
    for (i, a) in achados.iter().enumerate() {
        if i == 0 || achados[i - 1].caminho != a.caminho {
            fileiras.push(Fileira::Arquivo(i));
        }
        fileiras.push(Fileira::Achado(i));
    }
    fileiras
}

/// O trecho em uma linha, com o termo marcado (fundo âmbar).
fn trecho_marcado(texto: &str, termo: &str, maiusculas: bool, largura: f32) -> egui::text::LayoutJob {
    let p = cores();
    let fonte = FontId::monospace(11.5);
    let normal = egui::TextFormat::simple(fonte.clone(), p.texto);
    let marcado = egui::TextFormat { background: p.alerta.gamma_multiply(0.3), ..egui::TextFormat::simple(fonte, p.texto) };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = largura.max(20.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    for (parte, achou) in partes_do_termo(texto, termo, maiusculas) {
        job.append(parte, 0.0, if achou { marcado.clone() } else { normal.clone() });
    }
    job
}

/// Separa o texto nos pedaços com e sem o termo (sem diferenciar maiúsculas,
/// quando pedido). Só caracteres ASCII mudam de caixa, para os índices
/// continuarem os mesmos.
pub(crate) fn partes_do_termo<'a>(texto: &'a str, termo: &str, maiusculas: bool) -> Vec<(&'a str, bool)> {
    if termo.is_empty() {
        return vec![(texto, false)];
    }
    let (alvo, t) = if maiusculas { (texto.to_string(), termo.to_string()) } else { (texto.to_ascii_lowercase(), termo.to_ascii_lowercase()) };
    let mut partes = Vec::new();
    let mut inicio = 0;
    while let Some(i) = alvo[inicio..].find(&t) {
        let i = inicio + i;
        if i > inicio {
            partes.push((&texto[inicio..i], false));
        }
        partes.push((&texto[i..i + t.len()], true));
        inicio = i + t.len();
    }
    if inicio < texto.len() {
        partes.push((&texto[inicio..], false));
    }
    partes
}

/// O diff: o número de antes e o de depois, o sinal e o texto, com o fundo
/// verde no que entrou e vermelho no que saiu. Só as linhas visíveis.
fn linhas_do_diff(ui: &mut egui::Ui, linhas: &[api::LinhaDiff], cortado: bool, caminho: &str) {
    let p = cores();
    let area = ui.available_rect_before_wrap();
    ui.painter().rect(area, CornerRadius::same(tema::RAIO_CONTROLE), p.terminal_fundo, Stroke::new(1.0, p.borda), StrokeKind::Inside);
    let maior = linhas.iter().map(|l| l.antiga.max(l.nova)).max().unwrap_or(1);
    let calha = maior.to_string().len() as f32 * 7.5 + 12.0;
    let fonte = FontId::monospace(12.5);
    let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(vec2(0.0, 6.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    filho.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    egui::ScrollArea::both().id_salt(("diff", caminho)).auto_shrink(false).show_rows(
        &mut filho,
        LINHA_DIFF,
        linhas.len() + usize::from(cortado),
        |ui, faixa| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for n in faixa {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width().max(2.0 * calha + 24.0), LINHA_DIFF), Sense::hover());
                let Some(l) = linhas.get(n) else {
                    ui.painter().text(
                        pos2(r.left() + 12.0, r.center().y),
                        Align2::LEFT_CENTER,
                        "O diff continua: abra no editor para ver tudo",
                        FontId::proportional(12.0),
                        p.suave,
                    );
                    continue;
                };
                let (fundo, sinal, cor) = match l.tipo.as_str() {
                    "mais" => (Some(p.ok.gamma_multiply(0.14)), "+", p.terminal_texto),
                    "menos" => (Some(p.erro.gamma_multiply(0.14)), "−", p.terminal_texto),
                    "trecho" => (Some(p.realce.gamma_multiply(0.6)), "", p.suave),
                    _ => (None, "", p.terminal_texto),
                };
                if let Some(f) = fundo {
                    ui.painter().rect_filled(r, CornerRadius::ZERO, f);
                }
                let numero = |v: usize| if v == 0 { String::new() } else { v.to_string() };
                ui.painter().text(pos2(r.left() + calha - 4.0, r.center().y), Align2::RIGHT_CENTER, numero(l.antiga), fonte.clone(), p.suave);
                ui.painter().text(pos2(r.left() + 2.0 * calha - 4.0, r.center().y), Align2::RIGHT_CENTER, numero(l.nova), fonte.clone(), p.suave);
                ui.painter().text(pos2(r.left() + 2.0 * calha + 6.0, r.center().y), Align2::LEFT_CENTER, sinal, fonte.clone(), cor);
                let g = ui.painter().layout_no_wrap(l.texto.clone(), fonte.clone(), cor);
                let largura = g.size().x;
                ui.painter().galley(pos2(r.left() + 2.0 * calha + 20.0, r.center().y - g.size().y / 2.0), g, cor);
                if largura + 2.0 * calha + 20.0 > r.width() {
                    ui.allocate_exact_size(vec2(largura + 2.0 * calha + 20.0, 0.0), Sense::hover());
                }
            }
        },
    );
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn termo_marcado_sem_diferenciar_maiusculas() {
        assert_eq!(partes_do_termo("Total do Pedido", "pedido", false), vec![("Total do ", false), ("Pedido", true)]);
        assert_eq!(partes_do_termo("Total do Pedido", "pedido", true), vec![("Total do Pedido", false)]);
        assert_eq!(partes_do_termo("aXa", "a", false), vec![("a", true), ("X", false), ("a", true)]);
        // Acento não quebra os índices.
        assert_eq!(partes_do_termo("ação ação", "ção", false), vec![("a", false), ("ção", true), (" a", false), ("ção", true)]);
    }

    #[test]
    fn achados_agrupados_por_arquivo() {
        let a = |c: &str, l: usize| api::Achado { caminho: c.into(), linha: l, texto: String::new() };
        let f = agrupar(&[a("x.rs", 1), a("x.rs", 9), a("y.rs", 2)]);
        let tipos: Vec<char> = f.iter().map(|f| if matches!(f, Fileira::Arquivo(_)) { 'A' } else { 'l' }).collect();
        assert_eq!(tipos, vec!['A', 'l', 'l', 'A', 'l']);
    }
}
