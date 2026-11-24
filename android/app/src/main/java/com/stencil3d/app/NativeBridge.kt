package com.stencil3d.app

object NativeBridge {
    init {
        System.loadLibrary("stencil3d")
    }

    @JvmStatic external fun setImage(imageBytes: ByteArray): String
    @JvmStatic external fun processCached(paramsJson: String): String
    @JvmStatic external fun getMesh3d(): ByteArray
    @JvmStatic external fun exportStep(): ByteArray
    @JvmStatic external fun exportStl(): ByteArray
    @JvmStatic external fun exportSvg(): ByteArray
}
