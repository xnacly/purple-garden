# Keyword table lookup: a 500-arm linear chain of string equality checks
# against distinct literals, driven by 200000 runtime-built keys of which
# 1 in 6 miss and walk the full chain. Stresses compile-time string
# constants and runtime string equality, concat, and int-to-string.
# Loop form; CPython has no TCO. Same checksum as main.garden.
import sys


def classify(w):
    if w == "kw_0":
        return 0
    elif w == "kw_1":
        return 1
    elif w == "kw_2":
        return 2
    elif w == "kw_3":
        return 3
    elif w == "kw_4":
        return 4
    elif w == "kw_5":
        return 5
    elif w == "kw_6":
        return 6
    elif w == "kw_7":
        return 7
    elif w == "kw_8":
        return 8
    elif w == "kw_9":
        return 9
    elif w == "kw_10":
        return 10
    elif w == "kw_11":
        return 11
    elif w == "kw_12":
        return 12
    elif w == "kw_13":
        return 13
    elif w == "kw_14":
        return 14
    elif w == "kw_15":
        return 15
    elif w == "kw_16":
        return 16
    elif w == "kw_17":
        return 17
    elif w == "kw_18":
        return 18
    elif w == "kw_19":
        return 19
    elif w == "kw_20":
        return 20
    elif w == "kw_21":
        return 21
    elif w == "kw_22":
        return 22
    elif w == "kw_23":
        return 23
    elif w == "kw_24":
        return 24
    elif w == "kw_25":
        return 25
    elif w == "kw_26":
        return 26
    elif w == "kw_27":
        return 27
    elif w == "kw_28":
        return 28
    elif w == "kw_29":
        return 29
    elif w == "kw_30":
        return 30
    elif w == "kw_31":
        return 31
    elif w == "kw_32":
        return 32
    elif w == "kw_33":
        return 33
    elif w == "kw_34":
        return 34
    elif w == "kw_35":
        return 35
    elif w == "kw_36":
        return 36
    elif w == "kw_37":
        return 37
    elif w == "kw_38":
        return 38
    elif w == "kw_39":
        return 39
    elif w == "kw_40":
        return 40
    elif w == "kw_41":
        return 41
    elif w == "kw_42":
        return 42
    elif w == "kw_43":
        return 43
    elif w == "kw_44":
        return 44
    elif w == "kw_45":
        return 45
    elif w == "kw_46":
        return 46
    elif w == "kw_47":
        return 47
    elif w == "kw_48":
        return 48
    elif w == "kw_49":
        return 49
    elif w == "kw_50":
        return 50
    elif w == "kw_51":
        return 51
    elif w == "kw_52":
        return 52
    elif w == "kw_53":
        return 53
    elif w == "kw_54":
        return 54
    elif w == "kw_55":
        return 55
    elif w == "kw_56":
        return 56
    elif w == "kw_57":
        return 57
    elif w == "kw_58":
        return 58
    elif w == "kw_59":
        return 59
    elif w == "kw_60":
        return 60
    elif w == "kw_61":
        return 61
    elif w == "kw_62":
        return 62
    elif w == "kw_63":
        return 63
    elif w == "kw_64":
        return 64
    elif w == "kw_65":
        return 65
    elif w == "kw_66":
        return 66
    elif w == "kw_67":
        return 67
    elif w == "kw_68":
        return 68
    elif w == "kw_69":
        return 69
    elif w == "kw_70":
        return 70
    elif w == "kw_71":
        return 71
    elif w == "kw_72":
        return 72
    elif w == "kw_73":
        return 73
    elif w == "kw_74":
        return 74
    elif w == "kw_75":
        return 75
    elif w == "kw_76":
        return 76
    elif w == "kw_77":
        return 77
    elif w == "kw_78":
        return 78
    elif w == "kw_79":
        return 79
    elif w == "kw_80":
        return 80
    elif w == "kw_81":
        return 81
    elif w == "kw_82":
        return 82
    elif w == "kw_83":
        return 83
    elif w == "kw_84":
        return 84
    elif w == "kw_85":
        return 85
    elif w == "kw_86":
        return 86
    elif w == "kw_87":
        return 87
    elif w == "kw_88":
        return 88
    elif w == "kw_89":
        return 89
    elif w == "kw_90":
        return 90
    elif w == "kw_91":
        return 91
    elif w == "kw_92":
        return 92
    elif w == "kw_93":
        return 93
    elif w == "kw_94":
        return 94
    elif w == "kw_95":
        return 95
    elif w == "kw_96":
        return 96
    elif w == "kw_97":
        return 97
    elif w == "kw_98":
        return 98
    elif w == "kw_99":
        return 99
    elif w == "kw_100":
        return 100
    elif w == "kw_101":
        return 101
    elif w == "kw_102":
        return 102
    elif w == "kw_103":
        return 103
    elif w == "kw_104":
        return 104
    elif w == "kw_105":
        return 105
    elif w == "kw_106":
        return 106
    elif w == "kw_107":
        return 107
    elif w == "kw_108":
        return 108
    elif w == "kw_109":
        return 109
    elif w == "kw_110":
        return 110
    elif w == "kw_111":
        return 111
    elif w == "kw_112":
        return 112
    elif w == "kw_113":
        return 113
    elif w == "kw_114":
        return 114
    elif w == "kw_115":
        return 115
    elif w == "kw_116":
        return 116
    elif w == "kw_117":
        return 117
    elif w == "kw_118":
        return 118
    elif w == "kw_119":
        return 119
    elif w == "kw_120":
        return 120
    elif w == "kw_121":
        return 121
    elif w == "kw_122":
        return 122
    elif w == "kw_123":
        return 123
    elif w == "kw_124":
        return 124
    elif w == "kw_125":
        return 125
    elif w == "kw_126":
        return 126
    elif w == "kw_127":
        return 127
    elif w == "kw_128":
        return 128
    elif w == "kw_129":
        return 129
    elif w == "kw_130":
        return 130
    elif w == "kw_131":
        return 131
    elif w == "kw_132":
        return 132
    elif w == "kw_133":
        return 133
    elif w == "kw_134":
        return 134
    elif w == "kw_135":
        return 135
    elif w == "kw_136":
        return 136
    elif w == "kw_137":
        return 137
    elif w == "kw_138":
        return 138
    elif w == "kw_139":
        return 139
    elif w == "kw_140":
        return 140
    elif w == "kw_141":
        return 141
    elif w == "kw_142":
        return 142
    elif w == "kw_143":
        return 143
    elif w == "kw_144":
        return 144
    elif w == "kw_145":
        return 145
    elif w == "kw_146":
        return 146
    elif w == "kw_147":
        return 147
    elif w == "kw_148":
        return 148
    elif w == "kw_149":
        return 149
    elif w == "kw_150":
        return 150
    elif w == "kw_151":
        return 151
    elif w == "kw_152":
        return 152
    elif w == "kw_153":
        return 153
    elif w == "kw_154":
        return 154
    elif w == "kw_155":
        return 155
    elif w == "kw_156":
        return 156
    elif w == "kw_157":
        return 157
    elif w == "kw_158":
        return 158
    elif w == "kw_159":
        return 159
    elif w == "kw_160":
        return 160
    elif w == "kw_161":
        return 161
    elif w == "kw_162":
        return 162
    elif w == "kw_163":
        return 163
    elif w == "kw_164":
        return 164
    elif w == "kw_165":
        return 165
    elif w == "kw_166":
        return 166
    elif w == "kw_167":
        return 167
    elif w == "kw_168":
        return 168
    elif w == "kw_169":
        return 169
    elif w == "kw_170":
        return 170
    elif w == "kw_171":
        return 171
    elif w == "kw_172":
        return 172
    elif w == "kw_173":
        return 173
    elif w == "kw_174":
        return 174
    elif w == "kw_175":
        return 175
    elif w == "kw_176":
        return 176
    elif w == "kw_177":
        return 177
    elif w == "kw_178":
        return 178
    elif w == "kw_179":
        return 179
    elif w == "kw_180":
        return 180
    elif w == "kw_181":
        return 181
    elif w == "kw_182":
        return 182
    elif w == "kw_183":
        return 183
    elif w == "kw_184":
        return 184
    elif w == "kw_185":
        return 185
    elif w == "kw_186":
        return 186
    elif w == "kw_187":
        return 187
    elif w == "kw_188":
        return 188
    elif w == "kw_189":
        return 189
    elif w == "kw_190":
        return 190
    elif w == "kw_191":
        return 191
    elif w == "kw_192":
        return 192
    elif w == "kw_193":
        return 193
    elif w == "kw_194":
        return 194
    elif w == "kw_195":
        return 195
    elif w == "kw_196":
        return 196
    elif w == "kw_197":
        return 197
    elif w == "kw_198":
        return 198
    elif w == "kw_199":
        return 199
    elif w == "kw_200":
        return 200
    elif w == "kw_201":
        return 201
    elif w == "kw_202":
        return 202
    elif w == "kw_203":
        return 203
    elif w == "kw_204":
        return 204
    elif w == "kw_205":
        return 205
    elif w == "kw_206":
        return 206
    elif w == "kw_207":
        return 207
    elif w == "kw_208":
        return 208
    elif w == "kw_209":
        return 209
    elif w == "kw_210":
        return 210
    elif w == "kw_211":
        return 211
    elif w == "kw_212":
        return 212
    elif w == "kw_213":
        return 213
    elif w == "kw_214":
        return 214
    elif w == "kw_215":
        return 215
    elif w == "kw_216":
        return 216
    elif w == "kw_217":
        return 217
    elif w == "kw_218":
        return 218
    elif w == "kw_219":
        return 219
    elif w == "kw_220":
        return 220
    elif w == "kw_221":
        return 221
    elif w == "kw_222":
        return 222
    elif w == "kw_223":
        return 223
    elif w == "kw_224":
        return 224
    elif w == "kw_225":
        return 225
    elif w == "kw_226":
        return 226
    elif w == "kw_227":
        return 227
    elif w == "kw_228":
        return 228
    elif w == "kw_229":
        return 229
    elif w == "kw_230":
        return 230
    elif w == "kw_231":
        return 231
    elif w == "kw_232":
        return 232
    elif w == "kw_233":
        return 233
    elif w == "kw_234":
        return 234
    elif w == "kw_235":
        return 235
    elif w == "kw_236":
        return 236
    elif w == "kw_237":
        return 237
    elif w == "kw_238":
        return 238
    elif w == "kw_239":
        return 239
    elif w == "kw_240":
        return 240
    elif w == "kw_241":
        return 241
    elif w == "kw_242":
        return 242
    elif w == "kw_243":
        return 243
    elif w == "kw_244":
        return 244
    elif w == "kw_245":
        return 245
    elif w == "kw_246":
        return 246
    elif w == "kw_247":
        return 247
    elif w == "kw_248":
        return 248
    elif w == "kw_249":
        return 249
    elif w == "kw_250":
        return 250
    elif w == "kw_251":
        return 251
    elif w == "kw_252":
        return 252
    elif w == "kw_253":
        return 253
    elif w == "kw_254":
        return 254
    elif w == "kw_255":
        return 255
    elif w == "kw_256":
        return 256
    elif w == "kw_257":
        return 257
    elif w == "kw_258":
        return 258
    elif w == "kw_259":
        return 259
    elif w == "kw_260":
        return 260
    elif w == "kw_261":
        return 261
    elif w == "kw_262":
        return 262
    elif w == "kw_263":
        return 263
    elif w == "kw_264":
        return 264
    elif w == "kw_265":
        return 265
    elif w == "kw_266":
        return 266
    elif w == "kw_267":
        return 267
    elif w == "kw_268":
        return 268
    elif w == "kw_269":
        return 269
    elif w == "kw_270":
        return 270
    elif w == "kw_271":
        return 271
    elif w == "kw_272":
        return 272
    elif w == "kw_273":
        return 273
    elif w == "kw_274":
        return 274
    elif w == "kw_275":
        return 275
    elif w == "kw_276":
        return 276
    elif w == "kw_277":
        return 277
    elif w == "kw_278":
        return 278
    elif w == "kw_279":
        return 279
    elif w == "kw_280":
        return 280
    elif w == "kw_281":
        return 281
    elif w == "kw_282":
        return 282
    elif w == "kw_283":
        return 283
    elif w == "kw_284":
        return 284
    elif w == "kw_285":
        return 285
    elif w == "kw_286":
        return 286
    elif w == "kw_287":
        return 287
    elif w == "kw_288":
        return 288
    elif w == "kw_289":
        return 289
    elif w == "kw_290":
        return 290
    elif w == "kw_291":
        return 291
    elif w == "kw_292":
        return 292
    elif w == "kw_293":
        return 293
    elif w == "kw_294":
        return 294
    elif w == "kw_295":
        return 295
    elif w == "kw_296":
        return 296
    elif w == "kw_297":
        return 297
    elif w == "kw_298":
        return 298
    elif w == "kw_299":
        return 299
    elif w == "kw_300":
        return 300
    elif w == "kw_301":
        return 301
    elif w == "kw_302":
        return 302
    elif w == "kw_303":
        return 303
    elif w == "kw_304":
        return 304
    elif w == "kw_305":
        return 305
    elif w == "kw_306":
        return 306
    elif w == "kw_307":
        return 307
    elif w == "kw_308":
        return 308
    elif w == "kw_309":
        return 309
    elif w == "kw_310":
        return 310
    elif w == "kw_311":
        return 311
    elif w == "kw_312":
        return 312
    elif w == "kw_313":
        return 313
    elif w == "kw_314":
        return 314
    elif w == "kw_315":
        return 315
    elif w == "kw_316":
        return 316
    elif w == "kw_317":
        return 317
    elif w == "kw_318":
        return 318
    elif w == "kw_319":
        return 319
    elif w == "kw_320":
        return 320
    elif w == "kw_321":
        return 321
    elif w == "kw_322":
        return 322
    elif w == "kw_323":
        return 323
    elif w == "kw_324":
        return 324
    elif w == "kw_325":
        return 325
    elif w == "kw_326":
        return 326
    elif w == "kw_327":
        return 327
    elif w == "kw_328":
        return 328
    elif w == "kw_329":
        return 329
    elif w == "kw_330":
        return 330
    elif w == "kw_331":
        return 331
    elif w == "kw_332":
        return 332
    elif w == "kw_333":
        return 333
    elif w == "kw_334":
        return 334
    elif w == "kw_335":
        return 335
    elif w == "kw_336":
        return 336
    elif w == "kw_337":
        return 337
    elif w == "kw_338":
        return 338
    elif w == "kw_339":
        return 339
    elif w == "kw_340":
        return 340
    elif w == "kw_341":
        return 341
    elif w == "kw_342":
        return 342
    elif w == "kw_343":
        return 343
    elif w == "kw_344":
        return 344
    elif w == "kw_345":
        return 345
    elif w == "kw_346":
        return 346
    elif w == "kw_347":
        return 347
    elif w == "kw_348":
        return 348
    elif w == "kw_349":
        return 349
    elif w == "kw_350":
        return 350
    elif w == "kw_351":
        return 351
    elif w == "kw_352":
        return 352
    elif w == "kw_353":
        return 353
    elif w == "kw_354":
        return 354
    elif w == "kw_355":
        return 355
    elif w == "kw_356":
        return 356
    elif w == "kw_357":
        return 357
    elif w == "kw_358":
        return 358
    elif w == "kw_359":
        return 359
    elif w == "kw_360":
        return 360
    elif w == "kw_361":
        return 361
    elif w == "kw_362":
        return 362
    elif w == "kw_363":
        return 363
    elif w == "kw_364":
        return 364
    elif w == "kw_365":
        return 365
    elif w == "kw_366":
        return 366
    elif w == "kw_367":
        return 367
    elif w == "kw_368":
        return 368
    elif w == "kw_369":
        return 369
    elif w == "kw_370":
        return 370
    elif w == "kw_371":
        return 371
    elif w == "kw_372":
        return 372
    elif w == "kw_373":
        return 373
    elif w == "kw_374":
        return 374
    elif w == "kw_375":
        return 375
    elif w == "kw_376":
        return 376
    elif w == "kw_377":
        return 377
    elif w == "kw_378":
        return 378
    elif w == "kw_379":
        return 379
    elif w == "kw_380":
        return 380
    elif w == "kw_381":
        return 381
    elif w == "kw_382":
        return 382
    elif w == "kw_383":
        return 383
    elif w == "kw_384":
        return 384
    elif w == "kw_385":
        return 385
    elif w == "kw_386":
        return 386
    elif w == "kw_387":
        return 387
    elif w == "kw_388":
        return 388
    elif w == "kw_389":
        return 389
    elif w == "kw_390":
        return 390
    elif w == "kw_391":
        return 391
    elif w == "kw_392":
        return 392
    elif w == "kw_393":
        return 393
    elif w == "kw_394":
        return 394
    elif w == "kw_395":
        return 395
    elif w == "kw_396":
        return 396
    elif w == "kw_397":
        return 397
    elif w == "kw_398":
        return 398
    elif w == "kw_399":
        return 399
    elif w == "kw_400":
        return 400
    elif w == "kw_401":
        return 401
    elif w == "kw_402":
        return 402
    elif w == "kw_403":
        return 403
    elif w == "kw_404":
        return 404
    elif w == "kw_405":
        return 405
    elif w == "kw_406":
        return 406
    elif w == "kw_407":
        return 407
    elif w == "kw_408":
        return 408
    elif w == "kw_409":
        return 409
    elif w == "kw_410":
        return 410
    elif w == "kw_411":
        return 411
    elif w == "kw_412":
        return 412
    elif w == "kw_413":
        return 413
    elif w == "kw_414":
        return 414
    elif w == "kw_415":
        return 415
    elif w == "kw_416":
        return 416
    elif w == "kw_417":
        return 417
    elif w == "kw_418":
        return 418
    elif w == "kw_419":
        return 419
    elif w == "kw_420":
        return 420
    elif w == "kw_421":
        return 421
    elif w == "kw_422":
        return 422
    elif w == "kw_423":
        return 423
    elif w == "kw_424":
        return 424
    elif w == "kw_425":
        return 425
    elif w == "kw_426":
        return 426
    elif w == "kw_427":
        return 427
    elif w == "kw_428":
        return 428
    elif w == "kw_429":
        return 429
    elif w == "kw_430":
        return 430
    elif w == "kw_431":
        return 431
    elif w == "kw_432":
        return 432
    elif w == "kw_433":
        return 433
    elif w == "kw_434":
        return 434
    elif w == "kw_435":
        return 435
    elif w == "kw_436":
        return 436
    elif w == "kw_437":
        return 437
    elif w == "kw_438":
        return 438
    elif w == "kw_439":
        return 439
    elif w == "kw_440":
        return 440
    elif w == "kw_441":
        return 441
    elif w == "kw_442":
        return 442
    elif w == "kw_443":
        return 443
    elif w == "kw_444":
        return 444
    elif w == "kw_445":
        return 445
    elif w == "kw_446":
        return 446
    elif w == "kw_447":
        return 447
    elif w == "kw_448":
        return 448
    elif w == "kw_449":
        return 449
    elif w == "kw_450":
        return 450
    elif w == "kw_451":
        return 451
    elif w == "kw_452":
        return 452
    elif w == "kw_453":
        return 453
    elif w == "kw_454":
        return 454
    elif w == "kw_455":
        return 455
    elif w == "kw_456":
        return 456
    elif w == "kw_457":
        return 457
    elif w == "kw_458":
        return 458
    elif w == "kw_459":
        return 459
    elif w == "kw_460":
        return 460
    elif w == "kw_461":
        return 461
    elif w == "kw_462":
        return 462
    elif w == "kw_463":
        return 463
    elif w == "kw_464":
        return 464
    elif w == "kw_465":
        return 465
    elif w == "kw_466":
        return 466
    elif w == "kw_467":
        return 467
    elif w == "kw_468":
        return 468
    elif w == "kw_469":
        return 469
    elif w == "kw_470":
        return 470
    elif w == "kw_471":
        return 471
    elif w == "kw_472":
        return 472
    elif w == "kw_473":
        return 473
    elif w == "kw_474":
        return 474
    elif w == "kw_475":
        return 475
    elif w == "kw_476":
        return 476
    elif w == "kw_477":
        return 477
    elif w == "kw_478":
        return 478
    elif w == "kw_479":
        return 479
    elif w == "kw_480":
        return 480
    elif w == "kw_481":
        return 481
    elif w == "kw_482":
        return 482
    elif w == "kw_483":
        return 483
    elif w == "kw_484":
        return 484
    elif w == "kw_485":
        return 485
    elif w == "kw_486":
        return 486
    elif w == "kw_487":
        return 487
    elif w == "kw_488":
        return 488
    elif w == "kw_489":
        return 489
    elif w == "kw_490":
        return 490
    elif w == "kw_491":
        return 491
    elif w == "kw_492":
        return 492
    elif w == "kw_493":
        return 493
    elif w == "kw_494":
        return 494
    elif w == "kw_495":
        return 495
    elif w == "kw_496":
        return 496
    elif w == "kw_497":
        return 497
    elif w == "kw_498":
        return 498
    elif w == "kw_499":
        return 499
    return -1


def sweep(n):
    acc = 0
    for i in range(1, n + 1):
        acc += classify("kw_" + str(i % 600))
    return acc


if sweep(200000) != 41528550:
    sys.exit(1)
