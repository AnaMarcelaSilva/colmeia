//! Ajustes › Comandos falados e voz da abelha: cada item ligado à parte e
//! tudo desligado por padrão. Os comandos usam o Whisper (o mesmo do ditado),
//! a voz da abelha baixa o Kokoro, e o "ouvir o nome" deixa o microfone da
//! abelha fixa aberto, com a sensibilidade e, para um nome que não é o
//! padrão, 3 ou 4 gravações feitas aqui (que viram referências, não áudio).
//! A pergunta livre com IA é uma opção à parte no menu (o mesmo diálogo, só
//! com ela): é a única que usa tokens. O nome padrão também pode ser gravado
//! com a voz de quem usa (soma às referências sintéticas), e o teste mostra
//! ao vivo o quanto o que se fala parece o nome, para acertar a sensibilidade.

use std::sync::Arc;

use canal::voz::{Conversa, NomeOuvido, Saida};
use eframe::egui::{self, RichText};
use serde_json::json;

use crate::api;
use crate::dialogos::{Resultado, megabytes};
use crate::tema::{self, cores};

/// Uma sessão de gravação do nome, com um colmeia-voz só para ela.
struct Gravacao {
    conversa: Conversa,
    feitas: usize,
    ouvindo: bool,
}

/// O teste do nome: o microfone aberto e a última nota (0 a 100).
struct Teste {
    conversa: Conversa,
    nota: u8,
    limite: u8,
    /// Quando ouviu o nome pela última vez e com que nota (para mostrar
    /// "Ouvi!" e a nota de quando ouviu por um instante).
    ouviu_em: Option<f64>,
    nota_ouvida: u8,
    ouvindo: bool,
}

pub struct AjustesVoz {
    perfil: i64,
    pub opcoes: api::OpcoesVoz,
    abelha_fixa: bool,
    nome_abelha: String,
    modelos: ModeloWhisper,
    fala: api::EstadoVoz,
    nome: Option<NomeOuvido>,
    gravacao: Option<Gravacao>,
    teste: Option<Teste>,
    aviso: Option<String>,
    erro: Option<String>,
    /// Só a pergunta livre com IA (a opção à parte no menu).
    so_pergunta: bool,
}

impl AjustesVoz {
    /// A opção à parte: só a pergunta livre, sem perguntar dos modelos ao núcleo.
    pub fn pergunta(perfil: &api::Perfil) -> Self {
        AjustesVoz {
            perfil: perfil.id,
            opcoes: perfil.voz.clone(),
            abelha_fixa: perfil.abelha_fixa,
            nome_abelha: perfil.mascote_nome.clone(),
            modelos: ModeloWhisper { base: api::EstadoVoz::default(), small: api::EstadoVoz::default(), preferido: perfil.voz.voz_modelo.clone() },
            fala: api::EstadoVoz::default(),
            nome: None,
            gravacao: None,
            teste: None,
            aviso: None,
            erro: None,
            so_pergunta: true,
        }
    }

    pub fn new(perfil: &api::Perfil) -> Self {
        let mut erro = None;
        let mut ler = |r: Result<api::EstadoVoz, String>| {
            r.unwrap_or_else(|e| {
                erro = Some(format!("Não consegui falar com o núcleo: {e}"));
                api::EstadoVoz::default()
            })
        };
        let (modelos, erro_modelos) = ModeloWhisper::novo(&perfil.voz.voz_modelo);
        let fala = ler(api::estado_fala());
        let erro = erro.or(erro_modelos);
        AjustesVoz {
            perfil: perfil.id,
            opcoes: perfil.voz.clone(),
            abelha_fixa: perfil.abelha_fixa,
            nome_abelha: perfil.mascote_nome.clone(),
            modelos,
            fala,
            nome: canal::voz::nome_ouvido(perfil.id).ok(),
            gravacao: None,
            teste: None,
            aviso: None,
            erro,
            so_pergunta: false,
        }
    }

    /// O progresso de um download (pelo evento do núcleo).
    pub fn progresso(&mut self, pacote: &str, e: api::EstadoVoz) {
        match pacote {
            "kokoro" => self.fala = e,
            _ => self.modelos.progresso(pacote, e),
        }
    }

    fn mudar(&mut self, campos: serde_json::Value) {
        self.erro = api::definir_voz(self.perfil, campos).err().map(|e| format!("Não consegui mudar: {e}"));
    }
}

fn secao(ui: &mut egui::Ui, titulo: &str, texto: &str) {
    ui.add_space(16.0);
    ui.label(tema::texto_forte(titulo, 14.0).color(cores().texto));
    ui.add_space(3.0);
    ui.label(RichText::new(texto).size(12.5).color(cores().suave));
    ui.add_space(8.0);
}

fn barra(ui: &mut egui::Ui, e: &api::EstadoVoz) {
    let fracao = e.baixados as f32 / e.tamanho.max(1) as f32;
    ui.add(egui::ProgressBar::new(fracao).fill(cores().destaque).desired_height(8.0));
    ui.label(RichText::new(format!("{} de {}", megabytes(e.baixados), megabytes(e.tamanho))).color(cores().suave).size(12.0));
}

/// Mostra o diálogo; as mudanças vão ao núcleo e voltam pelo evento perfil.voz.
pub fn mostrar(ui: &mut egui::Ui, d: &mut AjustesVoz) -> Resultado {
    let mut resultado = Resultado::Continua;
    if d.so_pergunta {
        return mostrar_pergunta(ui, d);
    }
    receber_gravacao(ui.ctx(), d);
    receber_teste(ui.ctx(), d);
    tema::cabecalho(
        ui,
        "Comandos falados e voz da abelha",
        "Chame a abelha e ela diz, numerada, o que espera você e o que está trabalhando; responda com o número para abrir, responder ou concluir. Os comandos rodam neste computador, sem tokens, e o áudio não é gravado.",
    );
    egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
        comandos(ui, d);
        voz_da_abelha(ui, d);
        if ouvir_o_nome(ui, d) {
            resultado = Resultado::OuvirNome;
        }
    });
    if let Some(a) = &d.aviso {
        ui.add_space(8.0);
        ui.label(RichText::new(a).color(cores().suave).size(12.5));
    }
    if let Some(e) = &d.erro {
        ui.add_space(8.0);
        ui.label(RichText::new(e).color(cores().erro).size(12.5));
    }
    ui.add_space(16.0);
    if tema::botao_secundario(ui, "Fechar").clicked() {
        return Resultado::Fechar;
    }
    resultado
}

fn comandos(ui: &mut egui::Ui, d: &mut AjustesVoz) {
    secao(
        ui,
        "Comandos falados",
        &format!(
            "Clique na abelha (ou {}): ela mostra e fala a lista do que importa agora (\u{201c}1. teste: o Claude Code espera você\u{201d}) e o microfone abre. Diga o pedido com o número: \u{201c}abrir a um\u{201d}, \u{201c}responder a um\u{201d} (abre com o ditado ligado), \u{201c}concluir a dois\u{201d}, \u{201c}enviar para revisão a dois\u{201d}, \u{201c}rodar a três\u{201d} ou \u{201c}parar a três\u{201d} (o projeto da tarefa), \u{201c}próxima\u{201d}, \u{201c}repetir\u{201d} ou \u{201c}fechar\u{201d}. Os pedidos com o nome (\u{201c}rodar a loja web\u{201d}) também valem. Concluir e parar pedem confirmação. O resumo da abelha fica no botão direito.",
            crate::falada::ATALHO
        ),
    );
    let (pronto, baixando) = (d.modelos.pronto(), !d.modelos.pronto() && d.modelos.base.estado == "baixando");
    if baixando {
        barra(ui, &d.modelos.base);
    }
    let ligado = d.opcoes.voz_comandos;
    let rotulo = match (ligado, pronto, baixando) {
        (true, _, _) => "Desligar os comandos".to_string(),
        (_, _, true) => "Cancelar download".to_string(),
        (_, true, _) => "Ligar os comandos".to_string(),
        _ => format!("Baixar {} e ligar", megabytes(d.modelos.base.tamanho.max(1))),
    };
    if !pronto && !baixando && !ligado {
        ui.label(
            RichText::new("Os comandos usam o Whisper base (o mesmo do ditado), baixado uma vez do repositório oficial e conferido antes de usar.")
                .size(12.0)
                .color(cores().suave),
        );
        ui.add_space(6.0);
    }
    let clicou = if ligado || baixando { tema::botao_alerta(ui, &rotulo).clicked() } else { tema::botao_principal(ui, &rotulo, true).clicked() };
    if pronto && let Some(e) = d.modelos.mostrar(ui, d.perfil) {
        d.erro = Some(e);
    }
    if ligado {
        pausa(ui, d);
    }
    if !clicou {
        return;
    }
    d.erro = None;
    match (ligado, pronto, baixando) {
        (true, _, _) => d.mudar(json!({ "voz_comandos": false })),
        (_, _, true) => d.erro = api::cancelar_voz().err(),
        (_, true, _) => d.mudar(json!({ "voz_comandos": true })),
        _ => match api::baixar_voz_e_ligar(d.perfil, "comandos") {
            Ok(e) => d.modelos.base = e,
            Err(e) => d.erro = Some(format!("Não consegui começar o download: {e}")),
        },
    }
}

fn voz_da_abelha(ui: &mut egui::Ui, d: &mut AjustesVoz) {
    secao(ui, "Voz da abelha", "A resposta também é falada, com uma voz feminina em português (Kokoro, voz pf_dora), feita neste computador.");
    if d.fala.estado == "indisponivel" {
        ui.label(RichText::new("Ainda não disponível neste sistema: a resposta aparece só no balão.").size(12.5).color(cores().suave));
        return;
    }
    let (pronto, baixando) = (d.fala.estado == "pronto", d.fala.estado == "baixando");
    if baixando {
        barra(ui, &d.fala);
    }
    if d.fala.estado == "erro" && !d.fala.erro.is_empty() {
        ui.label(RichText::new(&d.fala.erro).color(cores().erro).size(12.5));
    }
    let ligada = d.opcoes.voz_abelha;
    let mut efeito = d.opcoes.voz_efeito;
    let (mut acao, mut apagar) = (false, false);
    ui.horizontal(|ui| {
        let rotulo = match (ligada, pronto, baixando) {
            (true, _, _) => "Desligar a voz".to_string(),
            (_, _, true) => "Cancelar download".to_string(),
            (_, true, _) => "Ligar a voz".to_string(),
            _ => format!("Baixar {} e ligar", megabytes(d.fala.tamanho.max(1))),
        };
        acao = if ligada || baixando { tema::botao_alerta(ui, &rotulo).clicked() } else { tema::botao_principal(ui, &rotulo, d.opcoes.voz_comandos).clicked() };
        if pronto && !baixando {
            apagar = tema::botao_secundario(ui, &format!("Apagar a voz ({})", megabytes(d.fala.tamanho)))
                .on_hover_text("Libera o espaço; a voz desliga em todos os perfis.")
                .clicked();
        }
    });
    if !pronto && !baixando && !ligada {
        ui.label(
            RichText::new("Baixa uma vez o modelo Kokoro-82M (8 bits), a voz e o ONNX Runtime, das fontes oficiais, conferidos antes de usar.")
                .size(12.0)
                .color(cores().suave),
        );
    }
    if ligada {
        ui.add_space(6.0);
        if tema::caixa_marcar(ui, "Efeito leve na voz (brilho e eco curto)", &mut efeito).changed() {
            d.mudar(json!({ "voz_efeito": efeito }));
        }
    }
    if apagar {
        match api::remover_fala() {
            Ok(e) => d.fala = e,
            Err(e) => d.erro = Some(format!("Não consegui apagar a voz: {e}")),
        }
    } else if acao {
        match (ligada, pronto, baixando) {
            (true, _, _) => d.mudar(json!({ "voz_abelha": false })),
            (_, _, true) => d.erro = api::cancelar_fala().err(),
            (_, true, _) => d.mudar(json!({ "voz_abelha": true })),
            _ => match api::baixar_fala(d.perfil) {
                Ok(e) => d.fala = e,
                Err(e) => d.erro = Some(format!("Não consegui começar o download: {e}")),
            },
        }
    }
}

/// Devolve `true` quando pediu para ligar (a janela abre a abelha fixa e liga).
fn ouvir_o_nome(ui: &mut egui::Ui, d: &mut AjustesVoz) -> bool {
    let nome = d.nome_abelha.clone();
    secao(
        ui,
        "Ouvir o nome (opcional)",
        &format!(
            "A abelha fixa fica ouvindo \u{201c}{nome}\u{201d} para ser chamada sem clique: diga o nome, ouça a lista e diga o pedido com o número. O microfone fica aberto o tempo todo, com a abelha de fone (o microfone aceso), e o som só passa pela memória."
        ),
    );
    let Some(n) = d.nome.clone() else {
        ui.label(RichText::new("Não consegui saber do núcleo como está o nome.").size(12.5).color(cores().suave));
        return false;
    };
    let pode_ouvir = n.padrao || n.gravado;
    if n.padrao && !n.gravado && d.gravacao.is_none() {
        ui.label(
            RichText::new(format!(
                "\u{201c}{nome}\u{201d} já vem com reconhecimento pronto, feito com voz sintética. Se ela não atender quando você chama, grave o nome com a sua voz."
            ))
            .size(12.5)
            .color(cores().texto),
        );
        ui.add_space(6.0);
        if tema::botao_secundario(ui, "Gravar o nome com a minha voz").clicked() {
            comecar_gravacao(ui.ctx(), d);
        }
    } else {
        gravar_o_nome(ui, d, &n);
    }
    ui.add_space(8.0);
    let ligado = d.opcoes.voz_nome;
    let rotulo = if ligado { "Parar de ouvir o nome" } else { "Ouvir o nome" };
    let pode = d.opcoes.voz_comandos && pode_ouvir;
    let clicou = if ligado { tema::botao_alerta(ui, rotulo).clicked() } else { tema::botao_principal(ui, rotulo, pode).clicked() };
    if !d.opcoes.voz_comandos {
        ui.label(RichText::new("Ligue os comandos falados primeiro.").size(12.0).color(cores().suave));
    } else if !d.abelha_fixa && !ligado {
        ui.label(RichText::new("Ligar fixa a abelha na área de trabalho: é ela que ouve.").size(12.0).color(cores().suave));
    }
    let mut ligar = false;
    if clicou {
        if ligado {
            d.mudar(json!({ "voz_nome": false }));
        } else {
            ligar = true;
            d.abelha_fixa = true;
        }
    }
    ui.add_space(10.0);
    let s = d.opcoes.voz_sensibilidade.clamp(1, 9);
    ui.label(RichText::new("Sensibilidade: mais alta ouve mais fácil (e se engana mais)").size(12.5).color(cores().texto));
    ui.add_space(4.0);
    const NIVEIS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
    if let Some(i) = tema::segmentado(ui, &NIVEIS, s as usize - 1) {
        let nova = i as u8 + 1;
        if nova != s {
            d.opcoes.voz_sensibilidade = nova;
            d.mudar(json!({ "voz_sensibilidade": nova }));
            // Testando, o teste segue com a sensibilidade nova.
            if let Some(t) = d.teste.as_mut() {
                t.conversa.cancelar();
                if let Err(e) = t.conversa.testar_nome(n.referencias().flatten(), nova) {
                    d.erro = Some(e);
                }
            }
        }
    }
    if pode_ouvir && d.opcoes.voz_comandos {
        testar_o_nome(ui, d, &n);
    }
    ligar
}

/// O teste: o microfone abre (só aqui, até parar ou 1 minuto) e a barra
/// mostra o quanto o que se fala parece o nome; o traço é o limite da
/// sensibilidade escolhida. Passou do traço, a abelha ouviria.
fn testar_o_nome(ui: &mut egui::Ui, d: &mut AjustesVoz, n: &NomeOuvido) {
    ui.add_space(10.0);
    let Some(t) = d.teste.as_mut() else {
        if tema::botao_secundario(ui, "Testar o nome").on_hover_text("Abre o microfone e mostra, enquanto você fala, se a abelha ouviria.").clicked() {
            let ctx = ui.ctx().clone();
            let mut conversa = Conversa::nova(d.perfil, Arc::new(move || ctx.request_repaint()));
            match conversa.testar_nome(n.referencias().flatten(), d.opcoes.voz_sensibilidade) {
                Ok(()) => d.teste = Some(Teste { conversa, nota: 0, limite: 50, ouviu_em: None, nota_ouvida: 0, ouvindo: true }),
                Err(e) => d.erro = Some(e),
            }
        }
        return;
    };
    let p = cores();
    let agora = ui.input(|i| i.time);
    let ouviu = t.ouviu_em.is_some_and(|q| agora - q < 1.5);
    let nota = if ouviu { t.nota_ouvida.max(t.nota) } else { t.nota };
    ui.horizontal(|ui| {
        // O microfone aberto fica sempre à vista.
        ui.label(RichText::new("● Microfone ligado").size(12.0).color(p.erro));
        let texto = if ouviu { format!("Ouvi \u{201c}{}\u{201d}!", n.nome) } else { format!("Diga \u{201c}{}\u{201d} e outras frases", n.nome) };
        ui.label(RichText::new(texto).size(12.5).color(if ouviu { p.ok } else { p.texto }));
    });
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(360.0), 12.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius::same(6), p.superficie);
    let cheia = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * nota as f32 / 100.0, rect.height()));
    ui.painter().rect_filled(cheia, egui::CornerRadius::same(6), if nota >= t.limite { p.ok } else { p.destaque });
    let x = rect.left() + rect.width() * t.limite as f32 / 100.0;
    ui.painter().line_segment([egui::pos2(x, rect.top() - 3.0), egui::pos2(x, rect.bottom() + 3.0)], egui::Stroke::new(2.0, p.texto));
    ui.label(RichText::new(format!("Parecido com o nome: {nota} · a abelha ouve a partir de {}", t.limite)).size(12.0).color(p.suave));
    if ouviu {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(1600));
    }
    if tema::botao_secundario(ui, "Parar o teste").clicked() || !t.ouvindo {
        t.conversa.cancelar();
        d.teste = None;
    }
}

fn receber_teste(_ctx: &egui::Context, d: &mut AjustesVoz) {
    let Some(t) = d.teste.as_mut() else { return };
    let agora = _ctx.input(|i| i.time);
    for s in t.conversa.receber() {
        match s {
            Saida::Nota { nota, limite, ouviu } => {
                t.nota = nota;
                t.limite = limite;
                if ouviu {
                    t.ouviu_em = Some(agora);
                    t.nota_ouvida = nota;
                }
            }
            Saida::Erro(e) => {
                t.ouvindo = false;
                d.aviso = Some(e);
            }
            _ => {}
        }
    }
}

/// A pergunta livre com IA: separada das outras e avisando que gasta tokens.
fn mostrar_pergunta(ui: &mut egui::Ui, d: &mut AjustesVoz) -> Resultado {
    tema::cabecalho(
        ui,
        "Pergunta livre com IA",
        "Pergunte à abelha sobre o andamento do trabalho, com as suas palavras. Desligada por padrão porque usa tokens.",
    );
    secao(
        ui,
        "Como funciona",
        "O que não for um dos comandos vira uma pergunta sobre o andamento, como \u{201c}o que eu fiz hoje?\u{201d} ou \u{201c}o que falta na sprint?\u{201d}, respondida pelo Claude Code com a conta deste perfil. Cada pergunta gasta tokens. Ele recebe só um retrato curto das tarefas, dos agentes, das frases da reunião e do que aconteceu hoje, nunca o terminal, senhas ou bancos, e nada é guardado. Dá para falar (com os comandos ligados) ou escrever no resumo da abelha.",
    );
    let ligada = d.opcoes.voz_ia;
    let clicou = if ligada {
        tema::botao_alerta(ui, "Desligar a pergunta livre").clicked()
    } else {
        tema::botao_principal(ui, "Ligar a pergunta livre", true).clicked()
    };
    if clicou {
        d.erro = None;
        d.mudar(json!({ "voz_ia": !ligada }));
    }
    if let Some(e) = &d.erro {
        ui.add_space(8.0);
        ui.label(RichText::new(e).color(cores().erro).size(12.5));
    }
    ui.add_space(16.0);
    if tema::botao_secundario(ui, "Fechar").clicked() {
        return Resultado::Fechar;
    }
    Resultado::Continua
}

/// Gravar o nome: 3 ou 4 gravações, uma de cada vez (no nome padrão, elas
/// somam às referências sintéticas).
fn gravar_o_nome(ui: &mut egui::Ui, d: &mut AjustesVoz, n: &NomeOuvido) {
    let nome = &n.nome;
    if n.gravado && d.gravacao.is_none() {
        ui.horizontal(|ui| {
            let extra = if n.padrao { " (somadas ao reconhecimento pronto)" } else { "" };
            ui.label(RichText::new(format!("As gravações de \u{201c}{nome}\u{201d} estão prontas{extra}.")).size(12.5).color(cores().texto));
            if tema::botao_secundario(ui, "Gravar de novo").clicked() {
                comecar_gravacao(ui.ctx(), d);
            }
            if tema::botao_secundario(ui, "Apagar as gravações").clicked() {
                match canal::voz::esquecer_nome(d.perfil) {
                    Ok(novo) => {
                        d.nome = Some(novo);
                        let ligado = d.opcoes.voz_nome && n.padrao;
                        d.mudar(json!({ "voz_nome": ligado }));
                    }
                    Err(e) => d.erro = Some(format!("Não consegui apagar: {e}")),
                }
            }
        });
        return;
    }
    let feitas = d.gravacao.as_ref().map_or(0, |g| g.feitas);
    let ouvindo = d.gravacao.as_ref().is_some_and(|g| g.ouvindo);
    ui.label(
        RichText::new(format!(
            "Para reconhecer \u{201c}{nome}\u{201d}, grave o nome {} ou {} vezes, uma de cada vez, do jeito que vai chamar. Gravações feitas: {feitas}.",
            3, 4
        ))
        .size(12.5)
        .color(cores().texto),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let rotulo = if ouvindo { "Ouvindo… diga o nome".to_string() } else { format!("Gravar ({} de 4)", (feitas + 1).min(4)) };
        if tema::botao_principal(ui, &rotulo, !ouvindo).clicked() {
            if d.gravacao.is_none() {
                comecar_gravacao(ui.ctx(), d);
            }
            if let Some(g) = d.gravacao.as_mut() {
                match g.conversa.amostra() {
                    Ok(()) => g.ouvindo = true,
                    Err(e) => d.erro = Some(e),
                }
            }
        }
        if feitas >= 3 && !ouvindo && tema::botao_principal(ui, "Aprender o nome", true).clicked() {
            let destino = n.arquivo.clone();
            if let Some(g) = d.gravacao.as_mut()
                && let Err(e) = g.conversa.aprender(nome, &destino)
            {
                d.erro = Some(e);
            }
        }
        if ouvindo {
            // O microfone aberto fica sempre à vista.
            ui.label(RichText::new("● Microfone ligado").size(12.0).color(cores().erro));
        }
    });
}

fn comecar_gravacao(ctx: &egui::Context, d: &mut AjustesVoz) {
    let ctx = ctx.clone();
    d.gravacao = Some(Gravacao { conversa: Conversa::nova(d.perfil, Arc::new(move || ctx.request_repaint())), feitas: 0, ouvindo: false });
    d.aviso = None;
}

fn receber_gravacao(_ctx: &egui::Context, d: &mut AjustesVoz) {
    let Some(g) = d.gravacao.as_mut() else { return };
    let mut aprendeu = false;
    for s in g.conversa.receber() {
        match s {
            Saida::Amostra(n) => {
                g.ouvindo = false;
                g.feitas = n;
            }
            Saida::Erro(e) => {
                g.ouvindo = false;
                d.aviso = Some(e);
            }
            Saida::Aprendido => aprendeu = true,
            _ => {}
        }
    }
    if aprendeu {
        // As gravações saem da memória junto com o programa.
        d.gravacao = None;
        d.nome = canal::voz::nome_ouvido(d.perfil).ok();
        d.aviso = Some("Pronto: o nome foi aprendido com a sua voz.".into());
        // Avisa a abelha fixa (pelo evento do perfil) para ouvir com as gravações novas.
        let ligado = d.opcoes.voz_nome;
        d.mudar(json!({ "voz_nome": ligado }));
    }
}

/// O Whisper que transcreve o ditado e os comandos: o base (padrão) ou o
/// small (maior e mais preciso), cada um baixado à parte e conferido pelo
/// núcleo. Vale o preferido do perfil se estiver baixado; senão, o outro.
pub struct ModeloWhisper {
    pub base: api::EstadoVoz,
    pub small: api::EstadoVoz,
    pub preferido: String,
}

impl ModeloWhisper {
    pub fn novo(preferido: &str) -> (ModeloWhisper, Option<String>) {
        let mut erro = None;
        let mut ler = |r: Result<api::EstadoVoz, String>| {
            r.unwrap_or_else(|e| {
                erro = Some(format!("Não consegui falar com o núcleo: {e}"));
                api::EstadoVoz::default()
            })
        };
        let base = ler(api::estado_voz());
        let small = ler(api::estado_small());
        (ModeloWhisper { base, small, preferido: preferido.to_string() }, erro)
    }

    /// O progresso de um download (pelo evento do núcleo).
    pub fn progresso(&mut self, pacote: &str, e: api::EstadoVoz) {
        match pacote {
            "whisper" => self.base = e,
            "small" => {
                if e.estado == "pronto" {
                    self.preferido = "small".into();
                }
                self.small = e;
            }
            _ => {}
        }
    }

    /// Há um Whisper no disco (o ditado e os comandos podem ligar).
    pub fn pronto(&self) -> bool {
        self.base.estado == "pronto" || self.small.estado == "pronto"
    }

    /// O que está valendo agora: o preferido baixado, ou o outro.
    pub fn em_uso(&self) -> &'static str {
        let small = self.small.estado == "pronto";
        let base = self.base.estado == "pronto";
        if small && (self.preferido == "small" || !base) { "small" } else { "base" }
    }

    /// A escolha entre os dois, com o download do small. Devolve um erro para mostrar.
    pub fn mostrar(&mut self, ui: &mut egui::Ui, perfil: i64) -> Option<String> {
        let mut erro = None;
        ui.add_space(10.0);
        ui.label(RichText::new("Modelo de transcrição").size(12.5).color(cores().texto));
        ui.add_space(4.0);
        let tamanho_small = megabytes(self.small.tamanho.max(487_601_967));
        let opcoes = [format!("Base · {}", megabytes(self.base.tamanho.max(147_951_465))), format!("Small · {tamanho_small}, mais preciso")];
        let rotulos: Vec<&str> = opcoes.iter().map(String::as_str).collect();
        let atual = usize::from(self.em_uso() == "small");
        if let Some(i) = tema::segmentado(ui, &rotulos, atual)
            && i != atual
        {
            let (nome, estado) = if i == 1 { ("small", &self.small) } else { ("base", &self.base) };
            if estado.estado == "pronto" {
                match api::definir_voz(perfil, json!({ "voz_modelo": nome })) {
                    Ok(()) => self.preferido = nome.into(),
                    Err(e) => erro = Some(format!("Não consegui trocar o modelo: {e}")),
                }
            } else if i == 1 && self.small.estado != "baixando" {
                match api::baixar_small(perfil) {
                    Ok(e) => self.small = e,
                    Err(e) => erro = Some(format!("Não consegui começar o download: {e}")),
                }
            }
        }
        let suave = |ui: &mut egui::Ui, t: &str| {
            ui.label(RichText::new(t).size(12.0).color(cores().suave));
        };
        match self.small.estado.as_str() {
            "baixando" => {
                barra(ui, &self.small);
                if tema::botao_secundario(ui, "Cancelar o download do small").clicked() {
                    erro = api::cancelar_small().err();
                }
            }
            "pronto" => {
                suave(ui, "O small acerta mais nomes e frases longas; transcreve em uns segundos a mais.");
                if tema::botao_secundario(ui, &format!("Apagar o small ({tamanho_small})")).on_hover_text("Libera o espaço; volta ao base.").clicked() {
                    match api::remover_small() {
                        Ok(e) => {
                            self.small = e;
                            self.preferido = "base".into();
                        }
                        Err(e) => erro = Some(format!("Não consegui apagar o small: {e}")),
                    }
                }
            }
            estado => {
                if estado == "erro" && !self.small.erro.is_empty() {
                    ui.label(RichText::new(&self.small.erro).color(cores().erro).size(12.5));
                }
                suave(ui, &format!("Escolher o small baixa {tamanho_small} uma vez do repositório oficial do whisper.cpp, conferidos antes de usar."));
            }
        }
        erro
    }
}

/// A pausa que encerra um comando falado, em segundos.
const PAUSAS: [(u32, &str); 5] = [(800, "0,8 s"), (1200, "1,2 s"), (1600, "1,6 s"), (2000, "2 s"), (2500, "2,5 s")];

fn pausa(ui: &mut egui::Ui, d: &mut AjustesVoz) {
    ui.add_space(10.0);
    ui.label(RichText::new("Silêncio que encerra o comando: mais longo espera você pensar no meio da frase").size(12.5).color(cores().texto));
    ui.add_space(4.0);
    let atual = PAUSAS.iter().position(|(ms, _)| *ms >= d.opcoes.voz_pausa).unwrap_or(1);
    let rotulos: Vec<&str> = PAUSAS.iter().map(|(_, r)| *r).collect();
    if let Some(i) = tema::segmentado(ui, &rotulos, atual)
        && i != atual
    {
        d.opcoes.voz_pausa = PAUSAS[i].0;
        d.mudar(json!({ "voz_pausa": PAUSAS[i].0 }));
    }
}
