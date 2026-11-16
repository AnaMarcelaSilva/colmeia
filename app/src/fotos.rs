//! As fotos das telas: cada teste desenha uma tela com dados fictícios num
//! harness do egui (egui_kittest) e grava o PNG em `app/tests/fotos/`.
//! Rodar `scripts/fotos.sh` depois de mexer numa tela e olhar as imagens
//! das telas tocadas; `UPDATE_SNAPSHOTS=true` grava as novas por cima.
//! Sem o núcleo: as imagens dos anexos ficam em branco, o resto é real.

use std::collections::HashSet;

use eframe::egui;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};

use crate::{api, registro, tema};

/// Um slide da daily com a montagem pronta.
#[allow(clippy::too_many_arguments)]
fn slide(
    id: i64,
    titulo: &str,
    coluna: &str,
    grupo: &str,
    projeto: &str,
    secao_id: i64,
    topico: &str,
    frase: &str,
    cartoes: &[(&str, &str)],
    apresentada: bool,
) -> api::Slide {
    api::Slide {
        tarefa_id: id,
        titulo: titulo.into(),
        projeto: projeto.into(),
        coluna: coluna.into(),
        grupo: grupo.into(),
        secao: projeto.into(),
        secao_id,
        partes: vec!["Hoje".into()],
        nota: frase.into(),
        apresentada,
        montagem: Some(api::Montagem {
            tarefa_id: id,
            topico: topico.into(),
            frase: frase.into(),
            cartoes: cartoes.iter().map(|(t, x)| api::Cartao { titulo: t.to_string(), texto: x.to_string(), ..Default::default() }).collect(),
            origem: "simples".into(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// A daily fictícia: dois projetos, seis tarefas, duas já apresentadas.
pub(crate) fn deck_daily() -> api::Deck {
    api::Deck {
        tipo: "daily".into(),
        titulo: "Daily · segunda, 16 de novembro".into(),
        periodo: "só hoje · todos os projetos · 2 projetos".into(),
        chave_nota: "2026-11-16".into(),
        apresentadas: 2,
        slides: vec![
            slide(
                76,
                "Botão para a reabertura ao ler comanda finalizada",
                "concluido",
                "concluidas",
                "loja-web",
                2,
                "Reabertura de comanda",
                "Ao ler uma comanda já finalizada, aparece um botão para reabrir a comanda.",
                &[
                    ("O que mudou", "Ao ler uma comanda já finalizada, aparece um botão para reabrir a comanda em vez da mensagem de erro."),
                    ("Endpoint novo", "PUT /comandas/{id}/reabrir na API em Go. Volta a comanda para \"aberta\" e grava quem reabriu e quando."),
                    ("Como testou", "Comanda fechada, reaberta pelo botão, item novo lançado e fechada de novo."),
                ],
                true,
            ),
            slide(
                77,
                "Mostrar a mesa no resumo da comanda",
                "concluido",
                "concluidas",
                "loja-web",
                2,
                "Mesa no resumo",
                "O resumo da comanda com a conta já impressa passa a mostrar a mesa no título.",
                &[
                    ("O que mudou", "O resumo mostra a mesa no título, como \"Itens · Mesa 5\"."),
                    ("De onde vem", "O número vem do campo mesa_id que a comanda já tinha."),
                ],
                true,
            ),
            slide(
                78,
                "Transferir comanda inteira para outra mesa",
                "concluido",
                "concluidas",
                "loja-web",
                2,
                "Transferência de comanda",
                "A comanda inteira agora pode ir para uma comanda nova da mesma mesa, e a mesa continua em atendimento.",
                &[
                    ("O que mudou", "A comanda inteira pode ir para uma comanda nova da mesma mesa."),
                    ("Sintoma e causa", "Ao transferir, a mesa ficava \"livre\" por um instante. A rotina fechava a mesa antes de abrir a comanda nova."),
                    (
                        "Como funciona",
                        "1. O operador escolhe Transferir na comanda.\n2. A API em Go cria a comanda nova na mesma mesa.\n3. Os itens mudam de comanda numa transação só.",
                    ),
                    ("Falta", "Subir para homologação e avisar o cliente."),
                ],
                false,
            ),
            slide(
                62,
                "Enviar para produção sem conferência",
                "concluido",
                "concluidas",
                "api-pedidos",
                1,
                "Produção sem conferência",
                "Um parâmetro novo manda o pedido direto para a produção, sem a tela de conferência.",
                &[
                    ("O que mudou", "Novo parâmetro \"Enviar para produção sem conferência\". Ligado, o Preparar manda direto."),
                    ("Onde fica", "Coluna nova producao_sem_conferencia na tabela de parâmetros da loja e o campo na tela de Configurações em React."),
                ],
                false,
            ),
            slide(
                80,
                "Endereço do cliente no pedido da loja",
                "trabalhando",
                "trabalhando",
                "api-pedidos",
                1,
                "Endereço no pedido",
                "Em andamento: o pedido da loja passa a trazer o endereço do cliente para a entrega.",
                &[
                    ("Onde está", "A API em Go já devolve o endereço no GET /pedidos/{id}. Falta a tela."),
                    ("Falta", "Mostrar o endereço no card do pedido e no cupom da entrega."),
                ],
                false,
            ),
            slide(
                88,
                "Travar quantidade acima do estoque",
                "aguardando",
                "aguardando",
                "api-pedidos",
                1,
                "Trava de estoque",
                "Esperando uma resposta: a trava vale para todos os itens ou só para os que controlam estoque?",
                &[("Onde está", "O agente perguntou se a trava vale para todos os itens ou só para os que controlam estoque.")],
                false,
            ),
        ],
        ..Default::default()
    }
}

/// O harness com as fontes e o tema da Colmeia. O primeiro quadro (o que o
/// harness roda ao nascer) só instala as fontes; a tela é desenhada a
/// partir do segundo, já com elas.
fn harness(largura: f32, altura: f32, mut tela: impl FnMut(&mut egui::Ui) + 'static) -> Harness<'static> {
    let mut pronto = false;
    let mut h = Harness::builder().with_size(egui::vec2(largura, altura)).with_theme(egui::Theme::Dark).build_ui(move |ui| {
        if !pronto {
            tema::instalar(ui.ctx());
            tema::Escolha::Escuro.aplicar(ui.ctx());
            pronto = true;
            return;
        }
        tela(ui);
    });
    h.run();
    h
}

/// Grava a foto em app/tests/fotos/<nome>.png.
fn foto(h: &mut Harness<'_>, nome: &str) {
    h.snapshot_options(nome, &SnapshotOptions::default().output_path("tests/fotos"));
}

#[test]
fn daily_leitor() {
    let mut r = registro::Registro::de_teste(1, api::Recorte::Perfil, Some(deck_daily()), None);
    let mut h = harness(1600.0, 900.0, move |ui| {
        r.mostrar(ui, registro::Aba::Daily, 1, api::Recorte::Perfil, &[], &[], &HashSet::new());
    });
    foto(&mut h, "daily-leitor");
}

#[test]
fn daily_leitor_tela_cheia() {
    let mut r = registro::Registro::de_teste(1, api::Recorte::Perfil, Some(deck_daily()), None);
    let mut h = harness(1600.0, 900.0, move |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, tema::cores().fundo);
        r.mostrar_cheio(ui, registro::Aba::Daily, 1, api::Recorte::Perfil);
    });
    foto(&mut h, "daily-leitor-tela-cheia");
}

/// O clique pelo nome estável do item (como a Key dos widget tests do
/// Flutter): a tarefa 62 vira a escolhida e o painel troca.
#[test]
fn daily_leitor_escolhe_pelo_nome() {
    let mut r = registro::Registro::de_teste(1, api::Recorte::Perfil, Some(deck_daily()), None);
    let mut h = harness(1600.0, 900.0, move |ui| {
        r.mostrar(ui, registro::Aba::Daily, 1, api::Recorte::Perfil, &[], &[], &HashSet::new());
    });
    h.get_by_label("tarefa 62").click();
    h.run();
    foto(&mut h, "daily-leitor-escolhida");
}

#[test]
fn sprint_leitor() {
    let mut deck = deck_daily();
    deck.tipo = "sprint".into();
    deck.titulo = "Sprint · 14/11 a 18/11".into();
    deck.periodo = "5 dias · todos os projetos · 2 projetos".into();
    deck.chave_nota = "2026-11-14..2026-11-18".into();
    deck.sprint_id = 3;
    deck.assuntos =
        vec![api::Assunto { id: 1, nome: "Comandas".into(), ..Default::default() }, api::Assunto { id: 2, nome: "Pedidos".into(), ..Default::default() }];
    for s in &mut deck.slides {
        s.assunto_id = if s.secao_id == 2 { 1 } else { 2 };
    }
    let mut r = registro::Registro::de_teste(1, api::Recorte::Perfil, None, Some(deck));
    let mut h = harness(1600.0, 900.0, move |ui| {
        r.mostrar(ui, registro::Aba::Sprint, 1, api::Recorte::Perfil, &[], &[], &HashSet::new());
    });
    foto(&mut h, "sprint-leitor");
}
