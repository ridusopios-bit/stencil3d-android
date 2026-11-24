package com.stencil3d.app

import android.content.Context
import android.graphics.*
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.*

class Preview3DView(context: Context) : View(context) {
    private var vx = FloatArray(0)
    private var vy = FloatArray(0)
    private var vz = FloatArray(0)
    private var t0 = IntArray(0)
    private var t1 = IntArray(0)
    private var t2 = IntArray(0)

    private var cx = 0f; private var cy = 0f; private var cz = 0f
    private var radius = 100f
    private var yaw = -32f
    private var pitch = 30f
    private var zoom = 1.05f
    private var lastX = 0f; private var lastY = 0f
    private var dragging = false
    private var status = "Выберите фото для 3D-предосмотра"

    private val paintTri = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL_AND_STROKE; strokeWidth = 1f }
    private val paintText = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.rgb(175, 185, 200)
        textSize = 30f
        textAlign = Paint.Align.CENTER
    }
    private val triPath = Path()

    private val scaleDetector = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(d: ScaleGestureDetector): Boolean {
            zoom = (zoom * d.scaleFactor).coerceIn(0.35f, 5f)
            invalidate()
            return true
        }
    })

    fun setStatus(s: String) {
        status = s
        vx = FloatArray(0); t0 = IntArray(0)
        invalidate()
    }

    fun resetCamera() {
        yaw = -32f; pitch = 30f; zoom = 1.05f
        invalidate()
    }

    fun setMeshBytes(bytes: ByteArray) {
        if (bytes.size < 8) {
            setStatus("Нет 3D-геометрии")
            return
        }
        val buf = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val nv = buf.int
        if (nv <= 0 || bytes.size < 8 + nv * 12) {
            setStatus("Ошибка чтения 3D-меша")
            return
        }
        val ax = FloatArray(nv); val ay = FloatArray(nv); val az = FloatArray(nv)
        var minX = Float.POSITIVE_INFINITY; var maxX = Float.NEGATIVE_INFINITY
        var minY = Float.POSITIVE_INFINITY; var maxY = Float.NEGATIVE_INFINITY
        var minZ = Float.POSITIVE_INFINITY; var maxZ = Float.NEGATIVE_INFINITY
        for (i in 0 until nv) {
            val x = buf.float; val y = buf.float; val z = buf.float
            ax[i] = x; ay[i] = y; az[i] = z
            if (x < minX) minX = x; if (x > maxX) maxX = x
            if (y < minY) minY = y; if (y > maxY) maxY = y
            if (z < minZ) minZ = z; if (z > maxZ) maxZ = z
        }
        val nt = buf.int
        val a0 = IntArray(nt); val a1 = IntArray(nt); val a2 = IntArray(nt)
        for (i in 0 until nt) {
            a0[i] = buf.int; a1[i] = buf.int; a2[i] = buf.int
        }
        vx = ax; vy = ay; vz = az; t0 = a0; t1 = a1; t2 = a2
        cx = 0.5f * (minX + maxX); cy = 0.5f * (minY + maxY); cz = 0.5f * (minZ + maxZ)
        radius = max(10f, 0.5f * sqrt((maxX - minX).pow(2) + (maxY - minY).pow(2) + (maxZ - minZ).pow(2)))
        status = ""
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        canvas.drawColor(Color.rgb(24, 27, 34))

        val nv = vx.size
        val nt = t0.size
        if (nv == 0 || nt == 0) {
            canvas.drawText(status, width * 0.5f, height * 0.5f, paintText)
            return
        }

        val ry = Math.toRadians(yaw.toDouble()).toFloat()
        val rp = Math.toRadians(pitch.toDouble()).toFloat()
        val cyR = cos(ry); val syR = sin(ry)
        val cpR = cos(rp); val spR = sin(rp)
        val sc = (min(width, height) * 0.44f / radius) * zoom
        val ox = width * 0.5f; val oy = height * 0.5f

        val px = FloatArray(nv); val py = FloatArray(nv); val pz = FloatArray(nv)
        val camX = FloatArray(nv); val camY = FloatArray(nv); val camZ = FloatArray(nv)

        for (i in 0 until nv) {
            val rx = vx[i] - cx; val ry0 = vy[i] - cy; val rz = vz[i] - cz
            val x1 = rx * cyR - ry0 * syR
            val y1 = rx * syR + ry0 * cyR
            val sx = x1
            val syUp = rz * cpR + y1 * spR
            val depth = -y1 * cpR + rz * spR
            camX[i] = sx; camY[i] = syUp; camZ[i] = depth
            px[i] = ox + sx * sc
            py[i] = oy - syUp * sc
            pz[i] = depth
        }

        val visIdx = IntArray(nt)
        val visZ = FloatArray(nt)
        var visCount = 0
        val step = if (dragging && nt > 6000) max(1, nt / 5000) else 1
        var i = 0
        while (i < nt) {
            val i0 = t0[i]; val i1 = t1[i]; val i2 = t2[i]
            val area2 = (px[i1] - px[i0]) * (py[i2] - py[i0]) - (py[i1] - py[i0]) * (px[i2] - px[i0])
            if (area2 < -0.1f) {
                visIdx[visCount] = i
                visZ[visCount] = (pz[i0] + pz[i1] + pz[i2]) * 0.3333f
                visCount++
            }
            i += step
        }

        val order = (0 until visCount).sortedBy { visZ[it] }
        for (k in order) {
            val ti = visIdx[k]
            val i0 = t0[ti]; val i1 = t1[ti]; val i2 = t2[ti]
            val ux = camX[i1] - camX[i0]; val uy = camY[i1] - camY[i0]; val uz = camZ[i1] - camZ[i0]
            val wx = camX[i2] - camX[i0]; val wy = camY[i2] - camY[i0]; val wz = camZ[i2] - camZ[i0]
            val nx = uy * wz - uz * wy
            val ny = uz * wx - ux * wz
            val nz = ux * wy - uy * wx
            val nLen = max(1e-6f, sqrt(nx * nx + ny * ny + nz * nz))
            val d1 = abs((-0.40f * nx + 0.55f * ny + 0.73f * nz) / nLen).coerceIn(0f, 1f)
            val d2 = abs((0.55f * nx - 0.35f * ny + 0.45f * nz) / nLen).coerceIn(0f, 1f)
            val sh = 0.28f + 0.56f * d1 + 0.22f * d2
            val r = min(255, (58f * sh + 16f * sh).toInt())
            val g = min(255, (64f * sh + 18f * sh).toInt())
            val b = min(255, (76f * sh + 22f * sh).toInt())
            paintTri.color = Color.rgb(r, g, b)

            triPath.reset()
            triPath.moveTo(px[i0], py[i0])
            triPath.lineTo(px[i1], py[i1])
            triPath.lineTo(px[i2], py[i2])
            triPath.close()
            canvas.drawPath(triPath, paintTri)
        }

        paintText.textSize = 24f
        canvas.drawText("1 палец: вращение  |  2 пальца: масштаб  |  Граней: $nt", width * 0.5f, height - 16f, paintText)
    }

    override fun onTouchEvent(ev: MotionEvent): Boolean {
        scaleDetector.onTouchEvent(ev)
        when (ev.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                dragging = true
                lastX = ev.x; lastY = ev.y
                parent?.requestDisallowInterceptTouchEvent(true)
            }
            MotionEvent.ACTION_MOVE -> {
                if (!scaleDetector.isInProgress && ev.pointerCount == 1) {
                    yaw += (ev.x - lastX) * 0.55f
                    pitch = (pitch + (ev.y - lastY) * 0.55f).coerceIn(-85f, 89f)
                    lastX = ev.x; lastY = ev.y
                    invalidate()
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                dragging = false
                invalidate()
            }
        }
        return true
    }
}
