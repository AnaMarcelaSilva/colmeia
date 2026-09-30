import 'package:flutter/scheduler.dart';

const nucleo = '127.0.0.1:7777';

/// Contadores globais, fora do estado dos widgets: incrementar não redesenha nada.
class Metricas {
  int bytes = 0;
  int quadros = 0;

  void iniciar() {
    // Conta os quadros que o Flutter realmente desenhou.
    SchedulerBinding.instance.addTimingsCallback((t) => quadros += t.length);
  }
}

final metricas = Metricas();
