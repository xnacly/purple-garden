const PREC = {
  cast: 1,
  compare: 2,
  add: 3,
  multiply: 4,
  unary: 5,
  call: 6,
};

module.exports = grammar({
  name: 'garden',

  extras: $ => [
    /[\s\uFEFF\u2060\u200B]+/,
    $.comment,
  ],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._item),

    _item: $ => choice(
      $.import_statement,
      $.let_statement,
      $.function_declaration,
      $.expression,
    ),

    comment: _ => token(seq('#', /.*/)),

    import_statement: $ => seq(
      'import',
      choice(
        $.string,
        seq('(', repeat1($.string), ')'),
      ),
    ),

    let_statement: $ => seq(
      'let',
      $.identifier,
      '=',
      $.expression,
    ),

    function_declaration: $ => seq(
      'fn',
      $.identifier,
      '(',
      repeat($.parameter),
      ')',
      optional($.type),
      $.block,
    ),

    parameter: $ => seq(
      $.identifier,
      ':',
      $.type,
    ),

    block: $ => seq(
      '{',
      repeat($._item),
      '}',
    ),

    match_expression: $ => seq(
      'match',
      '{',
      repeat($.match_arm),
      '}',
    ),

    match_arm: $ => choice(
      seq($.expression, $.block),
      $.block,
    ),

    expression: $ => choice(
      $.match_expression,
      $.cast_expression,
      $.comparison_expression,
      $.additive_expression,
      $.multiplicative_expression,
      $.unary_expression,
      $.postfix_expression,
      $.primary_expression,
    ),

    cast_expression: $ => prec.left(PREC.cast, seq(
      $.postfix_expression,
      'as',
      $.type,
    )),

    comparison_expression: $ => prec.left(PREC.compare, seq(
      $.additive_expression,
      choice('==', '!=', '<', '>'),
      $.additive_expression,
    )),

    additive_expression: $ => prec.left(PREC.add, seq(
      $.multiplicative_expression,
      choice('+', '-'),
      $.multiplicative_expression,
    )),

    multiplicative_expression: $ => prec.left(PREC.multiply, seq(
      $.unary_expression,
      choice('*', '/', '%'),
      $.unary_expression,
    )),

    unary_expression: $ => prec.right(PREC.unary, seq(
      choice('+', '-'),
      choice($.unary_expression, $.postfix_expression),
    )),

    postfix_expression: $ => prec.left(PREC.call, seq(
      $.primary_expression,
      repeat($.postfix_suffix),
    )),

    postfix_suffix: $ => choice(
      $.call_suffix,
      $.field_suffix,
    ),

    call_suffix: $ => seq(
      '(',
      repeat($.expression),
      ')',
    ),

    field_suffix: $ => seq(
      '.',
      $.identifier,
    ),

    primary_expression: $ => choice(
      $.identifier,
      $.string,
      $.number,
      $.boolean,
      seq('(', $.expression, ')'),
    ),

    boolean: $ => choice('true', 'false'),

    number: $ => token(choice(
      /\d+\.\d+/,
      /\d+/,
    )),

    string: $ => token(seq(
      '"',
      repeat(choice(
        /[^"\\\n]/,
        /\\./,
      )),
      '"',
    )),

    identifier: $ => /[A-Za-z_][A-Za-z0-9_]*/,

    type: $ => choice(
      $.type_atom,
      $.foreign_type,
      $.option_type,
      $.array_type,
    ),

    type_atom: $ => choice('Str', 'Int', 'Double', 'Bool', 'Void'),

    foreign_type: $ => seq(
      'Foreign',
      '<',
      $.type_identifier,
      '>',
    ),

    option_type: $ => seq(
      'Option',
      '<',
      $.type,
      '>',
    ),

    array_type: $ => seq(
      'Array',
      '<',
      $.type,
      '>',
    ),

    type_identifier: $ => $.identifier,
  },
});
